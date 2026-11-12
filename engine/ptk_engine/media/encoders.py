"""The compression algorithms: one encoder per codec.

Every encoder writes a new file next to nothing (``dst`` is in a temporary
folder); the pipeline checks the result before it replaces anything.
Quality is 1-100 (higher is better) and is mapped to each codec's own scale.
"""

from __future__ import annotations

import struct
import subprocess
import zlib
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from ..tools import Toolbox


class EncodeError(Exception):
    """The encoder failed; the original file is kept."""


class Unavailable(EncodeError):
    """No program for this codec is installed."""


#: Codecs each file extension can hold (the file keeps its name).
CONTAINERS: dict[str, tuple[str, ...]] = {
    ".png": ("png",),
    ".jpg": ("jpeg",),
    ".jpeg": ("jpeg",),
    ".webp": ("webp",),
    ".ogg": ("vorbis", "opus"),
    ".oga": ("vorbis", "opus"),
    ".opus": ("opus",),
    ".m4a": ("aac",),
    ".aac": ("aac",),
    ".mp3": ("mp3",),
    ".mp4": ("h264", "hevc", "av1"),
    ".m4v": ("h264", "hevc"),
    ".mov": ("h264", "hevc"),
    ".webm": ("vp9", "av1"),
    ".mkv": ("h264", "hevc", "vp9", "av1"),
}

#: The codec a file usually holds, for "keep each file's format".
USUAL = {".ogg": "vorbis", ".oga": "vorbis", ".mp4": "h264", ".m4v": "h264", ".mov": "h264",
         ".webm": "vp9", ".mkv": "h264"}

#: ffprobe codec names -> our codec ids.
PROBE_CODECS = {"vorbis": "vorbis", "opus": "opus", "aac": "aac", "mp3": "mp3", "h264": "h264",
                "hevc": "hevc", "vp9": "vp9", "av1": "av1"}

DEFAULT_QUALITY = {"textures": 80, "audio": 60, "video": 60}


def _clamp(v: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, v))


def vorbis_q(q: int) -> float:
    """1-100 -> libvorbis/oggenc -q (-1..10); 60 -> 4 (about 128 kbit/s)."""
    return round(_clamp(q / 12.5 - 0.8, -1, 10), 1)


def opus_kbps(q: int) -> int:
    return int(round(16 + q * 1.12))  # 60 -> 83


def aac_kbps(q: int) -> int:
    return int(round(32 + q * 1.6))  # 60 -> 128


def mp3_vbr(q: int) -> int:
    return int(round(_clamp((100 - q) * 0.09, 0, 9)))  # 60 -> V4


def jpeg_qscale(q: int) -> int:
    return int(round(_clamp(2 + (100 - q) * 0.29, 2, 31)))  # 80 -> 8


def video_crf(codec: str, q: int) -> int:
    base, span, top = {"h264": (18, 0.2, 51), "hevc": (20, 0.2, 51), "vp9": (20, 0.25, 63),
                       "av1": (22, 0.3, 63)}[codec]
    return int(round(_clamp(base + (100 - q) * span, 0, top)))


# -- helpers ------------------------------------------------------------------

def _run(args: list, what: str) -> None:
    try:
        r = subprocess.run([str(a) for a in args], capture_output=True, text=True, timeout=3600)
    except (OSError, subprocess.SubprocessError) as e:
        raise EncodeError(f"{what}: {e}") from e
    if r.returncode != 0:
        tail = (r.stderr or r.stdout).strip().splitlines()[-2:]
        raise EncodeError(f"{what} failed: " + " / ".join(tail))


_FFMPEG_ENCODERS: dict[str, set[str]] = {}


def ffmpeg_encoders(ffmpeg: str) -> set[str]:
    if ffmpeg not in _FFMPEG_ENCODERS:
        names: set[str] = set()
        try:
            out = subprocess.run([ffmpeg, "-hide_banner", "-encoders"], capture_output=True, text=True,
                                 timeout=30).stdout
            for line in out.splitlines():
                parts = line.split()
                if len(parts) >= 2 and len(parts[0]) == 6 and parts[0][0] in "VAS":
                    names.add(parts[1])
        except (OSError, subprocess.SubprocessError):
            pass
        _FFMPEG_ENCODERS[ffmpeg] = names
    return _FFMPEG_ENCODERS[ffmpeg]


def _ffmpeg_with(tb: Toolbox, lib: str) -> str | None:
    ff = tb.path("ffmpeg")
    return ff if ff and lib in ffmpeg_encoders(ff) else None


FF = ["-v", "error", "-nostdin", "-y"]
KEEP_TAGS = ["-map_metadata", "0", "-map_metadata:s:a:0", "0:s:a:0"]


# -- PNG -------------------------------------------------------------------------

def _chunks(data: bytes):
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise EncodeError("not a PNG file")
    i = 8
    while i + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[i:i + 8])
        yield kind, data[i + 8:i + 8 + length]
        i += 12 + length


def _chunk(kind: bytes, body: bytes) -> bytes:
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)


def redeflate_png(src: Path, dst: Path) -> None:
    """Lossless and always available: the same pixels, compressed harder.

    Only the IDAT stream is rewritten (zlib level 9, best of two
    strategies); every other chunk is copied as is, in order.
    """
    data = src.read_bytes()
    chunks = list(_chunks(data))
    idat = b"".join(body for kind, body in chunks if kind == b"IDAT")
    if not idat:
        raise EncodeError("PNG without image data")
    try:
        raw = zlib.decompress(idat)
    except zlib.error as e:
        raise EncodeError(f"broken PNG data: {e}") from e
    best = None
    for strategy in (zlib.Z_DEFAULT_STRATEGY, zlib.Z_FILTERED):
        c = zlib.compressobj(9, zlib.DEFLATED, 15, 9, strategy)
        out = c.compress(raw) + c.flush()
        if best is None or len(out) < len(best):
            best = out
    assert best is not None
    parts = [data[:8]]
    wrote = False
    for kind, body in chunks:
        if kind == b"IDAT":
            if not wrote:
                parts.extend(_chunk(b"IDAT", best[i:i + (1 << 20)]) for i in range(0, len(best), 1 << 20))
                wrote = True
            continue
        parts.append(_chunk(kind, body))
    dst.write_bytes(b"".join(parts))


def encode_png(src: Path, dst: Path, q: int | None, params: dict, tb: Toolbox) -> str:
    lossy = q is not None and q < 90 and tb.path("pngquant")
    current = src
    used = []
    if lossy:
        tmp = dst.with_name("quant.png")
        r = subprocess.run([tb.path("pngquant"), f"--quality=0-{q}", "--speed", "3", "--strip",
                            "--force", "--output", str(tmp), "--", str(src)], capture_output=True)
        if r.returncode == 0 and tmp.is_file():
            current = tmp
            used.append("pngquant")
        # 98/99: the result would be too big or too ugly; carry on losslessly.
    ox = tb.path("oxipng")
    if ox:
        _run([ox, "-o", "4", "--strip", "safe", "-q", "--out", dst, current], "oxipng")
        used.append("oxipng")
    else:
        redeflate_png(current, dst)
        used.append("deflate")
    return "+".join(used)


def png_backend(tb: Toolbox, q: int | None) -> str:
    parts = []
    if q is not None and q < 90 and tb.get("pngquant"):
        parts.append(f"pngquant {tb.version('pngquant')}")
    parts.append(f"oxipng {tb.version('oxipng')}" if tb.get("oxipng") else "deflate-9")
    return "+".join(parts)


# -- other images -----------------------------------------------------------------

def encode_jpeg(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
    ff = tb.path("ffmpeg")
    if not ff:
        raise Unavailable("ffmpeg")
    _run([ff, *FF, "-i", src, "-map_metadata", "-1", "-q:v", jpeg_qscale(q), "-frames:v", "1", dst], "ffmpeg")
    return "ffmpeg"


def encode_webp(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
    cw = tb.path("cwebp")
    if not cw:
        raise Unavailable("cwebp")
    _run([cw, "-quiet", "-q", q, "-alpha_q", "100", "-m", "6", "-mt", "-metadata", "icc", src, "-o", dst], "cwebp")
    return "cwebp"


# -- audio ---------------------------------------------------------------------------

def encode_vorbis(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
    ff = tb.path("ffmpeg")
    if not ff:
        raise Unavailable("ffmpeg")
    if "libvorbis" in ffmpeg_encoders(ff):
        _run([ff, *FF, "-i", src, "-map", "0:a:0", *KEEP_TAGS, "-c:a", "libvorbis",
              "-q:a", vorbis_q(q), dst], "ffmpeg")
        return "ffmpeg-libvorbis"
    og = tb.path("oggenc")
    if not og:
        raise Unavailable("oggenc")
    wav = dst.with_name("decoded.wav")
    _run([ff, *FF, "-i", src, "-map", "0:a:0", "-c:a", "pcm_s16le", wav], "ffmpeg")
    comments = []
    for k, v in (params.get("tags") or {}).items():
        if k.lower() not in ("encoder", "vendor", "encoded_by"):
            comments += ["-c", f"{k.upper()}={v}"]
    _run([og, "-Q", "-q", vorbis_q(q), *comments, "-o", dst, wav], "oggenc")
    wav.unlink(missing_ok=True)
    return "oggenc"


def encode_opus(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
    ff = _ffmpeg_with(tb, "libopus")
    if not ff:
        raise Unavailable("ffmpeg")
    _run([ff, *FF, "-i", src, "-map", "0:a:0", *KEEP_TAGS, "-c:a", "libopus", "-b:a", f"{opus_kbps(q)}k",
          "-f", "ogg" if dst.suffix in (".ogg", ".oga", ".opus") else "webm", dst], "ffmpeg")
    return "ffmpeg-libopus"


def encode_aac(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
    ff = tb.path("ffmpeg")
    lib = None
    if ff:
        encs = ffmpeg_encoders(ff)
        lib = "aac_at" if "aac_at" in encs else ("aac" if "aac" in encs else None)
    if ff and lib:
        _run([ff, *FF, "-i", src, "-map", "0:a:0", *KEEP_TAGS, "-c:a", lib, "-b:a", f"{aac_kbps(q)}k",
              "-movflags", "+faststart", dst], "ffmpeg")
        return f"ffmpeg-{lib}"
    af = tb.path("afconvert")
    if not af:
        raise Unavailable("ffmpeg")
    _run([af, "-f", "m4af" if dst.suffix == ".m4a" else "adts", "-d", "aac", "-b", aac_kbps(q) * 1000, src, dst],
         "afconvert")
    return "afconvert"


def encode_mp3(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
    ff = _ffmpeg_with(tb, "libmp3lame")
    if not ff:
        raise Unavailable("ffmpeg")
    _run([ff, *FF, "-i", src, "-map", "0:a:0", *KEEP_TAGS, "-c:a", "libmp3lame", "-q:a", mp3_vbr(q), dst], "ffmpeg")
    return "ffmpeg-lame"


# -- video ----------------------------------------------------------------------------

VIDEO_LIBS = {"h264": ("libx264",), "hevc": ("libx265",), "vp9": ("libvpx-vp9",), "av1": ("libsvtav1", "libaom-av1")}


def scale_filter(max_res: int) -> str:
    r = int(max_res)
    return (f"scale=w='if(gt(iw,ih),min(iw,{r}),-2)':h='if(gt(iw,ih),-2,min(ih,{r}))',"
            "scale=trunc(iw/2)*2:trunc(ih/2)*2")


def encode_video(codec: str):
    def run(src: Path, dst: Path, q: int, params: dict, tb: Toolbox) -> str:
        ff = tb.path("ffmpeg")
        lib = next((lib for lib in VIDEO_LIBS[codec] if ff and lib in ffmpeg_encoders(ff)), None)
        if not lib:
            raise Unavailable("ffmpeg")
        crf = video_crf(codec, q)
        args = [ff, *FF, "-i", src, "-map", "0:v:0", "-map", "0:a?", "-map_metadata", "0", "-c:a", "copy",
                "-c:v", lib, "-crf", crf, "-pix_fmt", "yuv420p"]
        if params.get("max_resolution"):
            args += ["-vf", scale_filter(params["max_resolution"])]
        if codec == "h264":
            args += ["-preset", "slow"]
        elif codec == "hevc":
            args += ["-preset", "medium", "-tag:v", "hvc1"]
        elif codec == "vp9":
            args += ["-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2"]
        elif lib == "libsvtav1":
            args += ["-preset", "8"]
        else:
            args += ["-b:v", "0", "-cpu-used", "6"]
        if dst.suffix in (".mp4", ".m4v", ".mov"):
            args += ["-movflags", "+faststart"]
        _run([*args, dst], "ffmpeg")
        return f"ffmpeg-{lib}"
    return run


@dataclass(frozen=True)
class Encoder:
    codec: str
    category: str
    run: Callable[..., str]
    tools: tuple[str, ...]  # what to install when it is missing (first is the main one)

    def backend(self, tb: Toolbox, q: int | None) -> str:
        """Which programs and versions would do the work (part of the cache key)."""
        if self.codec == "png":
            return png_backend(tb, q)
        if self.codec == "vorbis":
            ff = tb.path("ffmpeg")
            if ff and "libvorbis" in ffmpeg_encoders(ff):
                return f"ffmpeg {tb.version('ffmpeg')}"
            return f"ffmpeg {tb.version('ffmpeg')}+oggenc {tb.version('oggenc')}"
        return "+".join(f"{t} {tb.version(t)}" for t in self.tools if tb.get(t))

    def available(self, tb: Toolbox) -> bool:
        if self.codec == "png":
            return True
        if self.codec == "vorbis":
            ff = tb.path("ffmpeg")
            return bool(ff and ("libvorbis" in ffmpeg_encoders(ff) or tb.get("oggenc")))
        if self.codec == "aac":
            ff = tb.path("ffmpeg")
            return bool((ff and {"aac", "aac_at"} & ffmpeg_encoders(ff)) or tb.get("afconvert"))
        if self.codec == "webp":
            return bool(tb.get("cwebp"))
        ff = tb.path("ffmpeg")
        if not ff:
            return False
        need = {"opus": ("libopus",), "mp3": ("libmp3lame",), "jpeg": ("mjpeg",)}.get(self.codec) \
            or VIDEO_LIBS.get(self.codec, ())
        return any(n in ffmpeg_encoders(ff) for n in need)

    def missing_hint(self, tb: Toolbox) -> str:
        if self.codec == "vorbis":
            ff = tb.path("ffmpeg")
            if ff:
                return "FFmpeg has no libvorbis here; install vorbis-tools (brew install vorbis-tools)"
        tool = self.tools[0]
        from ..tools import TOOLS
        hint = TOOLS[tool].hint()
        return f"needs {TOOLS[tool].name}" + (f" ({hint})" if hint else "")


ENCODERS: dict[str, Encoder] = {e.codec: e for e in [
    Encoder("png", "textures", encode_png, ("oxipng",)),
    Encoder("jpeg", "textures", encode_jpeg, ("ffmpeg",)),
    Encoder("webp", "textures", encode_webp, ("cwebp",)),
    Encoder("vorbis", "audio", encode_vorbis, ("ffmpeg", "oggenc")),
    Encoder("opus", "audio", encode_opus, ("ffmpeg",)),
    Encoder("aac", "audio", encode_aac, ("ffmpeg",)),
    Encoder("mp3", "audio", encode_mp3, ("ffmpeg",)),
    Encoder("h264", "video", encode_video("h264"), ("ffmpeg",)),
    Encoder("hevc", "video", encode_video("hevc"), ("ffmpeg",)),
    Encoder("vp9", "video", encode_video("vp9"), ("ffmpeg",)),
    Encoder("av1", "video", encode_video("av1"), ("ffmpeg",)),
]}

#: Formats in the Rust list that only engines with a GPU texture pipeline
#: can use; loose image files are never turned into these.
GPU_TEXTURES = ("astc", "bc7", "etc2")
