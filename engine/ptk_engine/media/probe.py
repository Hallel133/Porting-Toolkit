"""What an image, sound or video file is, so re-encoding can be checked.

Images are read from their headers in pure Python (PNG, JPEG, WebP);
audio and video need ffprobe.
"""

from __future__ import annotations

import json
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ImageInfo:
    format: str
    width: int
    height: int
    alpha: bool


def image_info(path: Path) -> ImageInfo | None:
    try:
        with open(path, "rb") as f:
            head = f.read(64)
            if head.startswith(b"\x89PNG\r\n\x1a\n"):
                return _png(f)
            if head.startswith(b"\xff\xd8"):
                f.seek(0)
                return _jpeg(f.read())
            if head[:4] == b"RIFF" and head[8:12] == b"WEBP":
                return _webp(head)
    except (OSError, struct.error, ValueError):
        return None
    return None


def _png(f) -> ImageInfo | None:
    f.seek(8)
    width = height = 0
    alpha = False
    while True:
        hdr = f.read(8)
        if len(hdr) < 8:
            break
        length, kind = struct.unpack(">I4s", hdr)
        if kind == b"IHDR":
            data = f.read(length)
            width, height, _depth, color = struct.unpack(">IIBB", data[:10])
            alpha = color in (4, 6)
            f.seek(4, 1)
            continue
        if kind == b"tRNS":
            alpha = True
        if kind in (b"IDAT", b"IEND"):
            break
        f.seek(length + 4, 1)
    return ImageInfo("png", width, height, alpha) if width else None


def _jpeg(data: bytes) -> ImageInfo | None:
    i = 2
    while i + 9 < len(data):
        if data[i] != 0xFF:
            i += 1
            continue
        marker = data[i + 1]
        if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
            i += 2
            continue
        length = struct.unpack(">H", data[i + 2:i + 4])[0]
        if marker in (0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF):
            height, width = struct.unpack(">HH", data[i + 5:i + 9])
            return ImageInfo("jpeg", width, height, False)
        i += 2 + length
    return None


def _webp(head: bytes) -> ImageInfo | None:
    chunk = head[12:16]
    if chunk == b"VP8 ":
        w, h = struct.unpack("<HH", head[26:30])
        return ImageInfo("webp", w & 0x3FFF, h & 0x3FFF, False)
    if chunk == b"VP8L":
        b = head[21:25]
        bits = int.from_bytes(b, "little")
        return ImageInfo("webp", (bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1, bool((bits >> 28) & 1))
    if chunk == b"VP8X":
        flags = head[20]
        w = int.from_bytes(head[24:27], "little") + 1
        h = int.from_bytes(head[27:30], "little") + 1
        return ImageInfo("webp", w, h, bool(flags & 0x10))
    return None


@dataclass(frozen=True)
class MediaInfo:
    duration: float | None
    audio: str | None  # codec name of the first audio stream
    video: str | None
    width: int | None
    height: int | None
    channels: int | None
    tags: dict  # format and stream tags, lower-case keys (LOOPSTART -> loopstart)


def media_info(path: Path, ffprobe: str) -> MediaInfo | None:
    try:
        r = subprocess.run(
            [ffprobe, "-v", "error", "-show_entries",
             "format=duration:format_tags:stream=codec_type,codec_name,width,height,channels,duration:stream_tags",
             "-of", "json", str(path)],
            capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    if r.returncode != 0:
        return None
    try:
        data = json.loads(r.stdout)
    except ValueError:
        return None
    audio = video = None
    width = height = channels = None
    durations = []
    tags = {str(k).lower(): str(v) for k, v in ((data.get("format") or {}).get("tags") or {}).items()}
    for s in data.get("streams") or []:
        tags.update({str(k).lower(): str(v) for k, v in (s.get("tags") or {}).items()})
        if s.get("codec_type") == "audio" and audio is None:
            audio = s.get("codec_name")
            channels = s.get("channels")
        elif s.get("codec_type") == "video" and video is None and s.get("codec_name") not in ("png", "mjpeg"):
            video = s.get("codec_name")
            width, height = s.get("width"), s.get("height")
        if s.get("duration") not in (None, "N/A"):
            durations.append(float(s["duration"]))
    fmt = (data.get("format") or {}).get("duration")
    duration = float(fmt) if fmt not in (None, "N/A") else (max(durations) if durations else None)
    return MediaInfo(duration, audio, video, width, height, channels, tags)
