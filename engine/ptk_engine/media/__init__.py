"""The asset pipeline: applies a project's advanced options to an export.

Export plugins copy the game into their output and then call
``ctx.process_assets(root, profile)``. For each category whose option is
"Recompress", every file the engine can take is re-encoded in place with
the chosen (or the automatic) codec, then checked:

- the result must be at least 3% smaller,
- images keep their width, height and transparency,
- sounds and videos keep their length (within 1%), channels and loop tags
  (RPG Maker reads LOOPSTART/LOOPLENGTH from the files).

Anything that fails a check, or for which no program is installed, keeps
its original bytes. Results are cached by content, so the next export (or
another target with the same codec) is instant.

``mode="plan"`` changes nothing: it encodes the largest files of each
category and estimates the savings from them.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import tempfile
import threading
from collections import Counter
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from ..categories import classify
from ..scan import is_junk
from ..tools import Toolbox
from .encoders import CONTAINERS, DEFAULT_QUALITY, ENCODERS, GPU_TEXTURES, PROBE_CODECS, EncodeError, Unavailable
from .probe import image_info, media_info
from .profiles import PROFILES, Profile, decide

MEDIA_CATEGORIES = ("textures", "audio", "video")
MIN_SAVING = 0.03
PLAN_SAMPLE = 20
LOOP_TAGS = ("loopstart", "looplength", "loopend")


@dataclass
class Job:
    path: Path
    rel: str
    category: str
    codec: str
    size: int
    quality: int | None
    params: dict = field(default_factory=dict)
    source: object = None  # ImageInfo or MediaInfo
    #: The game's own file when ``path`` is a copy in an output that may
    #: already hold an earlier result (incremental builds).
    origin: Path | None = None

    @property
    def input(self) -> Path:
        return self.origin or self.path


@dataclass
class Outcome:
    status: str  # converted | kept | failed
    after: int
    cached: bool = False
    reason: str | None = None


@dataclass
class CategoryReport:
    files: int = 0
    converted: int = 0
    kept: int = 0
    failed: int = 0
    before: int = 0
    after: int = 0
    sampled: int = 0
    codecs: Counter = field(default_factory=Counter)
    notes: list[str] = field(default_factory=list)

    def note(self, text: str) -> None:
        if text and text not in self.notes and len(self.notes) < 8:
            self.notes.append(text)

    def to_dict(self) -> dict:
        return {"files": self.files, "converted": self.converted, "kept": self.kept, "failed": self.failed,
                "before": self.before, "after": self.after, "sampled": self.sampled,
                "codecs": dict(self.codecs), "notes": self.notes}


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class _Memo:
    """Hashes and probe results by (path, size, date), so an export that
    sees the same files again reads none of them."""

    def __init__(self, cache_dir: Path | None):
        self.file = cache_dir / "memo.json" if cache_dir else None
        self.data: dict = {}
        self.dirty = False
        if self.file and self.file.is_file():
            try:
                self.data = json.loads(self.file.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                self.data = {}
        self.lock = threading.Lock()

    @staticmethod
    def key(path: Path) -> str:
        st = path.stat()
        return f"{path}|{st.st_size}|{st.st_mtime_ns}"

    def get(self, path: Path, what: str, compute: Callable[[], object]):
        k = self.key(path)
        with self.lock:
            hit = self.data.get(k, {})
            if what in hit:
                return hit[what]
        value = compute()
        with self.lock:
            self.data.setdefault(k, {})[what] = value
            self.dirty = True
        return value

    def save(self) -> None:
        if not (self.file and self.dirty):
            return
        if len(self.data) > 50000:  # forget the oldest half
            self.data = dict(list(self.data.items())[len(self.data) // 2:])
        self.file.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.file.with_name("memo.json.tmp")
        tmp.write_text(json.dumps(self.data), encoding="utf-8")
        os.replace(tmp, self.file)


def _image(memo: _Memo, path: Path):
    from .probe import ImageInfo
    d = memo.get(path, "image", lambda: (lambda i: i and i.__dict__)(image_info(path)))
    return ImageInfo(**d) if d else None


def _media(memo: _Memo, path: Path, ffprobe: str):
    from .probe import MediaInfo
    d = memo.get(path, "media", lambda: (lambda i: i and dict(i.__dict__))(media_info(path, ffprobe)))
    return MediaInfo(**d) if d else None


def _same(a: Path, b: Path) -> bool:
    try:
        sa, sb = a.stat(), b.stat()
    except FileNotFoundError:
        return False
    return sa.st_size == sb.st_size and sa.st_mtime_ns == sb.st_mtime_ns


def _install(src: Path, dst: Path) -> None:
    """Puts ``src`` at ``dst`` unless it is already there (a clone keeps
    the date, so an unchanged result is not written again)."""
    if _same(src, dst):
        return
    from ..build.clone import copy_file
    part = dst.with_name(dst.name + ".ptk-part")
    copy_file(src, part)
    os.replace(part, dst)


def _files(root: Path):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if not d.startswith("."))
        for name in sorted(filenames):
            if is_junk(name) or name.startswith(".") or name.startswith("._"):
                continue
            p = Path(dirpath) / name
            if p.is_symlink() or not p.is_file():
                continue
            yield p, p.relative_to(root).as_posix()


class Pipeline:
    def __init__(self, toolbox: Toolbox, cache_dir: Path | None,
                 progress: Callable[[float | None, str], None] | None = None,
                 warn: Callable[[str], None] | None = None,
                 workers: int | None = None):
        self.tb = toolbox
        self.cache_dir = Path(cache_dir) if cache_dir else None
        self.progress = progress or (lambda f, m: None)
        self.warn = warn or (lambda m: None)
        self.workers = workers or max(1, min(8, os.cpu_count() or 2))
        self.cache_hits = 0
        self.memo = _Memo(self.cache_dir)
        self.source_of: Callable[[str], Path | None] | None = None

    # -- planning ---------------------------------------------------------------

    def _jobs(self, root: Path, profile: Profile, options: dict, target: dict,
              reports: dict[str, CategoryReport]) -> list[Job]:
        media = target.get("media") or {}
        ffprobe = self.tb.path("ffprobe")
        jobs: list[Job] = []
        for path, rel in _files(root):
            cat = classify(rel)
            rep = reports.get(cat)
            if rep is None:
                continue
            origin = self.source_of(rel) if self.source_of else None
            src = origin or path
            size = src.stat().st_size
            rep.files += 1
            rep.before += size
            rep.after += size
            opts = options[cat]
            action = opts.get("action", "keep")
            if action == "drop":
                rep.kept += 1
                rep.note(f"Files were kept: {profile.name} games load them by name, so they can't be dropped.")
                continue
            if cat not in profile.categories:
                rep.kept += 1
                continue
            ext = path.suffix.lower()
            if ext not in CONTAINERS:
                rep.kept += 1
                rep.note(f"{ext or 'files without an extension'} can't be re-encoded in place; kept as is.")
                continue
            source = None
            source_codec = None
            if cat == "textures":
                source = _image(self.memo, src)
                if source is None:
                    rep.kept += 1
                    continue
            else:
                if not ffprobe:
                    rep.kept += 1
                    rep.note("Sound and video need FFmpeg (brew install ffmpeg); kept as is.")
                    continue
                source = _media(self.memo, src, ffprobe)
                stream = (source.audio if cat == "audio" else source.video) if source else None
                if not source or not stream:
                    rep.kept += 1
                    continue
                source_codec = PROBE_CODECS.get(stream)
                if source_codec is None:
                    rep.kept += 1
                    rep.note(f"{ext} files with {stream} can't be re-encoded here; kept.")
                    continue
            requested = opts.get("format")
            if requested in GPU_TEXTURES and cat == "textures":
                rep.note(f"{requested.upper()} is a GPU texture format; {profile.name} loads image files, "
                         "so they stay PNG/JPEG/WebP.")
                requested = None
            preferred = list(media.get(cat) or [])
            decision = decide(profile, cat, ext, requested, preferred, source_codec)
            if decision.note:
                rep.note(decision.note)
            if not decision.codec:
                rep.kept += 1
                continue
            enc = ENCODERS[decision.codec]
            if not enc.available(self.tb):
                rep.kept += 1
                rep.note(f"{decision.codec}: {enc.missing_hint(self.tb)}; files kept as they are.")
                continue
            q = opts.get("quality")
            if q is None and not (cat == "textures" and decision.codec == "png"):
                q = DEFAULT_QUALITY[cat]
            params: dict = {}
            if cat == "video" and opts.get("max_resolution"):
                params["max_resolution"] = int(opts["max_resolution"])
            if cat == "audio" and source is not None:
                params["tags"] = {k: v for k, v in getattr(source, "tags", {}).items()}
            if cat == "textures" and opts.get("max_resolution"):
                rep.note(f"Images keep their size: {profile.name} games rely on exact pixel sizes.")
            jobs.append(Job(path, rel, cat, decision.codec, size, q, params, source, origin))
        return jobs

    # -- one file -------------------------------------------------------------

    def _key(self, job: Job, digest: str) -> str:
        enc = ENCODERS[job.codec]
        spec = json.dumps({"codec": job.codec, "q": job.quality, "backend": enc.backend(self.tb, job.quality),
                           "max_resolution": job.params.get("max_resolution"), "v": 1}, sort_keys=True)
        return hashlib.sha256(f"{digest}|{spec}".encode()).hexdigest()

    def _verify(self, job: Job, out: Path) -> str | None:
        if job.category == "textures":
            src, dst = job.source, image_info(out)
            if dst is None:
                return "the result is not a readable image"
            if (src.width, src.height) != (dst.width, dst.height):
                return f"size changed ({src.width}x{src.height} -> {dst.width}x{dst.height})"
            if src.alpha and not dst.alpha:
                return "transparency was lost"
            return None
        ffprobe = self.tb.path("ffprobe")
        dst = media_info(out, ffprobe) if ffprobe else None
        src = job.source
        if dst is None:
            return "the result is not readable"
        if src.duration and dst.duration is not None:
            if abs(src.duration - dst.duration) > max(0.1 if job.category == "video" else 0.05, src.duration * 0.01):
                return f"length changed ({src.duration:.2f}s -> {dst.duration:.2f}s)"
        if job.category == "audio":
            if not dst.audio:
                return "no sound in the result"
            if src.channels and dst.channels and src.channels != dst.channels:
                return "channel count changed"
            for tag in LOOP_TAGS:
                if tag in src.tags and dst.tags.get(tag) != src.tags[tag]:
                    return f"loop tag {tag.upper()} was lost"
        else:
            if not dst.video:
                return "no video in the result"
            if src.width and dst.width:
                if job.params.get("max_resolution"):
                    if dst.width > src.width or dst.height > src.height:
                        return "the video got bigger"
                    if abs(src.width / src.height - dst.width / dst.height) > 0.02 * (src.width / src.height):
                        return "aspect ratio changed"
                elif (src.width, src.height) != (dst.width, dst.height):
                    return "size changed"
            if src.audio and not dst.audio:
                return "the sound track was lost"
        return None

    def encode(self, job: Job) -> tuple[Outcome, Path | None]:
        """Encodes one file into the cache (or finds it there)."""
        digest = self.memo.get(job.input, "sha256", lambda: _sha256(job.input))
        key = self._key(job, digest)
        ext = job.path.suffix.lower()
        cached = skip = None
        if self.cache_dir:
            folder = self.cache_dir / key[:2]
            cached, skip = folder / f"{key}{ext}", folder / f"{key}.skip"
            if cached.is_file():
                self.cache_hits += 1
                return Outcome("converted", cached.stat().st_size, cached=True), cached
            if skip.is_file():
                self.cache_hits += 1
                reason = skip.read_text(encoding="utf-8", errors="replace").strip() or None
                status = "failed" if reason and not reason.startswith("not smaller") else "kept"
                return Outcome(status, job.size, cached=True, reason=reason), None
        tmp = Path(tempfile.mkdtemp(prefix="ptk-media-"))
        try:
            out = tmp / f"out{ext}"
            try:
                ENCODERS[job.codec].run(job.input, out, job.quality, job.params, self.tb)
            except Unavailable as e:
                return Outcome("kept", job.size, reason=f"missing {e}"), None
            except EncodeError as e:
                return self._remember(skip, Outcome("failed", job.size, reason=str(e))), None
            if not out.is_file():
                return self._remember(skip, Outcome("failed", job.size, reason="no output")), None
            problem = self._verify(job, out)
            if problem:
                return self._remember(skip, Outcome("failed", job.size, reason=problem)), None
            after = out.stat().st_size
            if after > job.size * (1 - MIN_SAVING):
                return self._remember(skip, Outcome("kept", job.size, reason="not smaller")), None
            if cached is None:
                keep = Path(tempfile.mkdtemp(prefix="ptk-media-out-")) / out.name
                shutil.move(str(out), keep)
                return Outcome("converted", after), keep
            cached.parent.mkdir(parents=True, exist_ok=True)
            part = cached.with_name(cached.name + ".part")
            shutil.move(str(out), part)
            os.replace(part, cached)
            return Outcome("converted", after), cached
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    @staticmethod
    def _remember(skip: Path | None, outcome: Outcome) -> Outcome:
        if skip is not None and outcome.reason and not outcome.reason.startswith("missing"):
            skip.parent.mkdir(parents=True, exist_ok=True)
            skip.write_text(outcome.reason + "\n", encoding="utf-8")
        return outcome

    # -- the whole folder -------------------------------------------------------

    def run(self, root: Path, profile_id: str, options: dict, target: dict, mode: str = "apply",
            source_of: Callable[[str], Path | None] | None = None) -> dict | None:
        """Processes ``root`` in place (``apply``) or estimates (``plan``).

        ``source_of(rel)`` gives the game's own file for an output file
        (incremental builds): it is read instead of the output copy, which
        may hold the result of an earlier export. Returns None when every
        media option is "keep".
        """
        self.source_of = source_of
        active = {c: o for c, o in (options or {}).items()
                  if isinstance(o, dict) and o.get("action", "keep") != "keep"}
        if not active or mode == "off":
            return None
        profile = PROFILES.get(profile_id) or PROFILES["html5"]
        reports = {c: CategoryReport() for c in active}
        for c in active:
            if c not in MEDIA_CATEGORIES:
                reports[c].note("Re-encoding this kind of file is not supported yet; kept as is.")
        media_opts = {c: o for c, o in active.items()}
        if profile.untouched:
            for r in reports.values():
                r.note(profile.untouched)
            return self._report(profile, target, mode, reports)
        self.progress(None, "Looking at the game's files")
        jobs = self._jobs(Path(root), profile, media_opts, target, reports)
        if mode == "plan":
            by_cat: dict[str, list[Job]] = {}
            for j in jobs:
                by_cat.setdefault(j.category, []).append(j)
            todo = []
            for cat, js in by_cat.items():
                js.sort(key=lambda j: j.size, reverse=True)
                todo += js[:PLAN_SAMPLE]
        else:
            todo = jobs
        results = self._execute(todo, mode)
        self.memo.save()
        if mode == "plan":
            self._estimate(jobs, todo, results, reports)
        else:
            for job, (outcome, _) in zip(todo, results):
                self._count(reports[job.category], job, outcome)
        return self._report(profile, target, mode, reports)

    def _report(self, profile: Profile, target: dict, mode: str, reports: dict[str, CategoryReport]) -> dict:
        before = sum(r.before for r in reports.values())
        after = sum(r.after for r in reports.values())
        return {
            "profile": profile.id,
            "profile_name": profile.name,
            "target": target.get("id"),
            "mode": mode,
            "estimated": mode == "plan" and not profile.untouched,
            "categories": {c: r.to_dict() for c, r in reports.items()},
            "before": before,
            "after": after,
            "cache_hits": self.cache_hits,
        }

    def _execute(self, jobs: list[Job], mode: str) -> list[tuple[Outcome, Path | None]]:
        results: list[tuple[Outcome, Path | None] | None] = [None] * len(jobs)
        total = len(jobs)
        done = 0
        lock = threading.Lock()
        verb = "Estimating" if mode == "plan" else "Re-encoding"

        def one(i: int) -> None:
            nonlocal done
            job = jobs[i]
            try:
                outcome, path = self.encode(job)
            except OSError as e:
                outcome, path = Outcome("failed", job.size, reason=str(e)), None
            if mode == "apply" and outcome.status == "converted" and path is not None:
                _install(path, job.path)
                if self.cache_dir is None:
                    shutil.rmtree(path.parent, ignore_errors=True)
            elif mode == "apply" and job.origin is not None:
                _install(job.origin, job.path)  # an earlier result is there: back to the original
            elif path is not None and self.cache_dir is None:
                shutil.rmtree(path.parent, ignore_errors=True)
            results[i] = (outcome, path)
            with lock:
                done += 1
                self.progress(done / total if total else 1.0, f"{verb} {done}/{total}: {job.rel}")

        light = [i for i, j in enumerate(jobs) if j.category != "video"]
        heavy = [i for i, j in enumerate(jobs) if j.category == "video"]
        with ThreadPoolExecutor(max_workers=self.workers) as pool:
            list(pool.map(one, light))
        for i in heavy:  # FFmpeg already uses every core for video
            one(i)
        return [r for r in results if r is not None]

    @staticmethod
    def _count(rep: CategoryReport, job: Job, outcome: Outcome) -> None:
        if outcome.status == "converted":
            rep.converted += 1
            rep.codecs[job.codec] += 1
            rep.after -= job.size - outcome.after
        elif outcome.status == "failed":
            rep.failed += 1
            if outcome.reason:
                rep.note(f"{job.rel}: {outcome.reason}; kept the original.")
        else:
            rep.kept += 1
            if outcome.reason and outcome.reason.startswith("missing"):
                rep.note(f"{job.codec}: {outcome.reason}; kept the original.")

    def _estimate(self, jobs: list[Job], sample: list[Job], results, reports) -> None:
        sampled: dict[str, tuple[int, int, int, int]] = {}
        for job, (outcome, _) in zip(sample, results):
            b, a, conv, n = sampled.get(job.category, (0, 0, 0, 0))
            sampled[job.category] = (b + job.size, a + outcome.after, conv + (outcome.status == "converted"), n + 1)
            if outcome.status == "failed" and outcome.reason:
                reports[job.category].note(f"{job.rel}: {outcome.reason}")
        totals: dict[str, tuple[int, int]] = {}
        for j in jobs:
            size, n = totals.get(j.category, (0, 0))
            totals[j.category] = (size + j.size, n + 1)
        for cat, (b, a, conv, n) in sampled.items():
            rep = reports[cat]
            size, count = totals[cat]
            ratio = a / b if b else 1.0
            rep.after = rep.before - size + int(size * ratio)
            rep.converted = round(conv / n * count) if n else 0
            rep.kept += count - rep.converted
            rep.sampled = n
            for j in jobs:
                if j.category == cat:
                    rep.codecs[j.codec] += 1


def describe(report: dict | None) -> str:
    """One line for logs: '12 files re-encoded, 48.0 MB -> 31.2 MB'."""
    if not report:
        return ""
    conv = sum(c["converted"] for c in report["categories"].values())
    mb = lambda n: f"{n / 1e6:.1f} MB"  # noqa: E731
    est = "about " if report.get("estimated") else ""
    return f"{est}{conv} files re-encoded, {mb(report['before'])} -> {est}{mb(report['after'])}"
