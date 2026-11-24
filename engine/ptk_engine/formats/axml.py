"""Android binary XML (AndroidManifest.xml) and resources.arsc string pools.

Enough to retarget a prebuilt APK, the way Godot's own exporter does
(``platform/android/export/export_plugin.cpp``): change the package name,
version, orientation and other attributes of the compiled manifest, add
``<uses-permission>`` elements, and replace strings (the app name) in
``resources.arsc``.

Chunk types: string pool 0x0001, XML 0x0003, resource map 0x0180, start
tag 0x0102, end tag 0x0103. Strings are rewritten as UTF-16; new strings are
appended, so the resource map (which covers the first strings) stays valid.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field

RES_STRING_POOL = 0x0001
RES_TABLE = 0x0002
RES_XML = 0x0003
RES_XML_START_NAMESPACE = 0x0100
RES_XML_END_NAMESPACE = 0x0101
RES_XML_START_ELEMENT = 0x0102
RES_XML_END_ELEMENT = 0x0103
RES_XML_RESOURCE_MAP = 0x0180

UTF8_FLAG = 0x100
NO_INDEX = 0xFFFFFFFF
ANDROID_NS = "http://schemas.android.com/apk/res/android"

TYPE_STRING = 0x03
TYPE_INT_DEC = 0x10
TYPE_INT_BOOLEAN = 0x12

#: android:name, the attribute every <uses-permission> has.
ATTR_NAME = 0x01010003


class AxmlError(Exception):
    pass


# -- string pools ------------------------------------------------------------------


@dataclass
class StringPool:
    strings: list[str]
    styles: bytes = b""  # style spans, kept as they are (manifests have none)
    style_count: int = 0
    utf8: bool = False

    @classmethod
    def parse(cls, data: bytes, offset: int) -> tuple[StringPool, int]:
        """Parses the pool chunk at ``offset``; returns it and its size."""
        ctype, header_size, size = struct.unpack_from("<HHI", data, offset)
        if ctype != RES_STRING_POOL:
            raise AxmlError(f"expected a string pool at {offset}, found chunk 0x{ctype:04x}")
        count, style_count, flags, strings_start, styles_start = struct.unpack_from("<IIIII", data, offset + 8)
        utf8 = bool(flags & UTF8_FLAG)
        offsets = struct.unpack_from(f"<{count}I", data, offset + header_size)
        base = offset + strings_start
        out = []
        for o in offsets:
            out.append(_read_string(data, base + o, utf8))
        styles = b""
        if style_count:
            styles = data[offset + styles_start:offset + size]
        return cls(out, styles, style_count, utf8), size

    def index(self, s: str) -> int:
        try:
            return self.strings.index(s)
        except ValueError:
            return -1

    def add(self, s: str) -> int:
        """Appends a string (never reusing one, so no other user changes)."""
        self.strings.append(s)
        return len(self.strings) - 1

    def build(self) -> bytes:
        if self.style_count:
            raise AxmlError("string pools with styles are not supported")
        header_size = 28
        offsets = []
        body = bytearray()
        for s in self.strings:
            offsets.append(len(body))
            units = s.encode("utf-16-le")
            n = len(units) // 2
            if n > 0x7FFF:
                body += struct.pack("<HH", 0x8000 | (n >> 16), n & 0xFFFF)
            else:
                body += struct.pack("<H", n)
            body += units + b"\0\0"
        while len(body) % 4:
            body += b"\0"
        strings_start = header_size + 4 * len(offsets)
        size = strings_start + len(body)
        head = struct.pack("<HHIIIIII", RES_STRING_POOL, header_size, size, len(self.strings), 0, 0,
                           strings_start, 0)
        return head + struct.pack(f"<{len(offsets)}I", *offsets) + bytes(body)


def _read_string(data: bytes, pos: int, utf8: bool) -> str:
    if utf8:
        # UTF-16 length (1 or 2 bytes), then UTF-8 length (1 or 2 bytes).
        n = data[pos]
        pos += 2 if n & 0x80 else 1
        n = data[pos]
        if n & 0x80:
            n = ((n & 0x7F) << 8) | data[pos + 1]
            pos += 2
        else:
            pos += 1
        return data[pos:pos + n].decode("utf-8", "replace")
    n = struct.unpack_from("<H", data, pos)[0]
    pos += 2
    if n & 0x8000:
        n = ((n & 0x7FFF) << 16) | struct.unpack_from("<H", data, pos)[0]
        pos += 2
    return data[pos:pos + 2 * n].decode("utf-16-le", "replace")


# -- binary XML --------------------------------------------------------------------


@dataclass
class Attribute:
    ns: int
    name: int
    raw: int
    type: int
    data: int


@dataclass
class Node:
    """One XML chunk. Start tags are decoded; everything else stays raw."""

    ctype: int
    raw: bytes = b""
    line: int = 0
    ns: int = NO_INDEX
    name: int = NO_INDEX
    attrs: list[Attribute] = field(default_factory=list)
    ids: tuple[int, int, int] = (0, 0, 0)  # id, class and style attribute indexes (1-based, 0 = none)


class Manifest:
    """A compiled AndroidManifest.xml that can be edited and written back."""

    def __init__(self, data: bytes):
        ctype, header_size, size = struct.unpack_from("<HHI", data, 0)
        if ctype != RES_XML:
            raise AxmlError("not a binary XML file")
        pos = header_size
        self.pool, n = StringPool.parse(data, pos)
        pos += n
        self.resource_map: list[int] = []
        self.nodes: list[Node] = []
        while pos < len(data):
            ctype, chs, csize = struct.unpack_from("<HHI", data, pos)
            if csize < 8:
                raise AxmlError("broken chunk")
            chunk = data[pos:pos + csize]
            if ctype == RES_XML_RESOURCE_MAP:
                self.resource_map = list(struct.unpack_from(f"<{(csize - chs) // 4}I", chunk, chs))
            elif ctype == RES_XML_START_ELEMENT:
                line, _comment = struct.unpack_from("<II", chunk, 8)
                ns, name, astart, asize, count, idi, cli, sti = struct.unpack_from("<IIHHHHHH", chunk, 16)
                attrs = []
                for i in range(count):
                    a = 16 + astart + i * asize
                    ans, aname, raw, _vsize, _res0, atype, adata = struct.unpack_from("<IIIHBBI", chunk, a)
                    attrs.append(Attribute(ans, aname, raw, atype, adata))
                self.nodes.append(Node(ctype, line=line, ns=ns, name=name, attrs=attrs, ids=(idi, cli, sti)))
            else:
                self.nodes.append(Node(ctype, raw=chunk))
            pos += csize

    # -- reading -------------------------------------------------------------------

    def s(self, i: int) -> str | None:
        return self.pool.strings[i] if i != NO_INDEX and i < len(self.pool.strings) else None

    def name_of(self, node: Node) -> str | None:
        if node.ctype == RES_XML_START_ELEMENT:
            return self.s(node.name)
        if node.ctype == RES_XML_END_ELEMENT:
            return self.s(struct.unpack_from("<I", node.raw, 20)[0])
        return None

    def elements(self, tag: str):
        return [n for n in self.nodes if n.ctype == RES_XML_START_ELEMENT and self.s(n.name) == tag]

    def attr(self, node: Node, name: str) -> Attribute | None:
        return next((a for a in node.attrs if self.s(a.name) == name), None)

    def value(self, node: Node, name: str):
        a = self.attr(node, name)
        if a is None:
            return None
        if a.type == TYPE_STRING:
            return self.s(a.data)
        if a.raw != NO_INDEX:
            return self.s(a.raw)
        return a.data

    def to_dict(self) -> dict:
        """A small summary, for checks and tests."""
        out: dict = {}
        man = self.elements("manifest")[0]
        out["package"] = self.value(man, "package")
        out["versionCode"] = self.value(man, "versionCode")
        out["versionName"] = self.value(man, "versionName")
        out["permissions"] = [self.value(n, "name") for n in self.elements("uses-permission")]
        out["activities"] = [self.value(n, "name") for n in self.elements("activity")]
        out["providers"] = [self.value(n, "authorities") for n in self.elements("provider")]
        return out

    # -- editing -------------------------------------------------------------------

    def set_string(self, node: Node, name: str, value: str) -> bool:
        a = self.attr(node, name)
        if a is None:
            return False
        i = self.pool.index(value)
        if i < 0:
            i = self.pool.add(value)
        a.raw, a.type, a.data = i, TYPE_STRING, i
        return True

    def set_int(self, node: Node, name: str, value: int, boolean: bool = False) -> bool:
        a = self.attr(node, name)
        if a is None:
            return False
        a.raw = NO_INDEX
        a.type = TYPE_INT_BOOLEAN if boolean else TYPE_INT_DEC
        a.data = (0xFFFFFFFF if value else 0) if boolean else value & 0xFFFFFFFF
        return True

    def _android_name_index(self) -> int:
        """The "name" string that the resource map ties to android:name."""
        for i, rid in enumerate(self.resource_map):
            if rid == ATTR_NAME and self.pool.strings[i] == "name":
                return i
        raise AxmlError("the manifest has no android:name attribute string")

    def add_permission(self, permission: str) -> None:
        if permission in self.to_dict()["permissions"]:
            return
        ns = self.pool.index(ANDROID_NS)
        if ns < 0:
            raise AxmlError("the manifest has no android namespace")
        name_attr = self._android_name_index()
        tag = self.pool.index("uses-permission")
        if tag < 0:
            tag = self.pool.add("uses-permission")
        value = self.pool.add(permission)
        start = Node(RES_XML_START_ELEMENT, line=1, ns=NO_INDEX, name=tag,
                     attrs=[Attribute(ns, name_attr, value, TYPE_STRING, value)])
        end = Node(RES_XML_END_ELEMENT, raw=struct.pack("<HHIIIII", RES_XML_END_ELEMENT, 16, 24, 1, NO_INDEX,
                                                       NO_INDEX, tag))
        # After the last <uses-permission>, else right after <manifest ...>
        # (before <application>, as aapt writes them).
        ends = [i for i, n in enumerate(self.nodes) if n.ctype == RES_XML_END_ELEMENT
                and self.name_of(n) == "uses-permission"]
        if ends:
            at = ends[-1] + 1
        else:
            at = next(i for i, n in enumerate(self.nodes) if n.ctype == RES_XML_START_ELEMENT
                      and self.s(n.name) == "manifest") + 1
        self.nodes[at:at] = [start, end]

    def build(self) -> bytes:
        out = bytearray(self.pool.build())
        if self.resource_map:
            out += struct.pack("<HHI", RES_XML_RESOURCE_MAP, 8, 8 + 4 * len(self.resource_map))
            out += struct.pack(f"<{len(self.resource_map)}I", *self.resource_map)
        for n in self.nodes:
            if n.ctype != RES_XML_START_ELEMENT:
                out += n.raw
                continue
            body = struct.pack("<IIHHHHHH", n.ns, n.name, 20, 20, len(n.attrs), *n.ids)
            for a in n.attrs:
                body += struct.pack("<IIIHBBI", a.ns, a.name, a.raw, 8, 0, a.type, a.data)
            out += struct.pack("<HHIII", RES_XML_START_ELEMENT, 16, 16 + len(body), n.line, NO_INDEX) + body
        return struct.pack("<HHI", RES_XML, 8, 8 + len(out)) + bytes(out)


# -- resources.arsc ------------------------------------------------------------------


def replace_table_strings(arsc: bytes, replace) -> bytes:
    """Rewrites the global string pool of a resources.arsc, passing each
    string through ``replace(s) -> s`` (Godot replaces its
    ``godot-project-name`` strings with the app's name this way)."""
    ctype, header_size, size, _packages = struct.unpack_from("<HHII", arsc, 0)
    if ctype != RES_TABLE:
        raise AxmlError("not a resources.arsc file")
    pool, pool_size = StringPool.parse(arsc, header_size)
    if pool.style_count:
        raise AxmlError("the resource table's strings have styles; not supported")
    pool.strings = [replace(s) for s in pool.strings]
    pool.utf8 = False
    new_pool = pool.build()
    rest = arsc[header_size + pool_size:]
    body = arsc[:header_size] + new_pool + rest
    return body[:4] + struct.pack("<I", len(body)) + body[8:]


def table_strings(arsc: bytes) -> list[str]:
    _ctype, header_size, _size, _ = struct.unpack_from("<HHII", arsc, 0)
    return StringPool.parse(arsc, header_size)[0].strings


# -- building small manifests (for tests) -----------------------------------------------


def build_manifest(package: str, activity: str, attrs: dict | None = None, provider: str | None = None) -> bytes:
    """A compiled manifest the way aapt lays it out (tests use it)."""
    strings = ["versionCode", "versionName", "name", "label", "screenOrientation", "authorities",
               "android", ANDROID_NS, "", "manifest", "package", package, "1.0", "application", "activity",
               activity, "provider", provider or "", "app"]
    res_map = [0x0101021B, 0x0101021C, ATTR_NAME, 0x01010001, 0x0101001E, 0x01010018]
    pool = StringPool(strings)
    idx = {s: i for i, s in reversed(list(enumerate(strings)))}
    ns = idx[ANDROID_NS]
    m = Manifest.__new__(Manifest)
    m.pool = pool
    m.resource_map = res_map
    m.nodes = []

    def start(tag, attributes):
        m.nodes.append(Node(RES_XML_START_ELEMENT, line=1, name=idx[tag], attrs=attributes))

    def end(tag):
        m.nodes.append(Node(RES_XML_END_ELEMENT, raw=struct.pack("<HHIIIII", RES_XML_END_ELEMENT, 16, 24, 1,
                                                               NO_INDEX, NO_INDEX, idx[tag])))

    m.nodes.append(Node(RES_XML_START_NAMESPACE, raw=struct.pack("<HHIIIII", RES_XML_START_NAMESPACE, 16, 24, 1,
                                                               NO_INDEX, idx["android"], ns)))
    start("manifest", [Attribute(ns, idx["versionCode"], NO_INDEX, TYPE_INT_DEC, 1),
                       Attribute(ns, idx["versionName"], idx["1.0"], TYPE_STRING, idx["1.0"]),
                       Attribute(NO_INDEX, idx["package"], idx[package], TYPE_STRING, idx[package])])
    start("application", [Attribute(ns, idx["label"], idx["app"], TYPE_STRING, idx["app"])])
    start("activity", [Attribute(ns, idx["name"], idx[activity], TYPE_STRING, idx[activity]),
                       Attribute(ns, idx["screenOrientation"], NO_INDEX, TYPE_INT_DEC, 0)])
    end("activity")
    if provider:
        start("provider", [Attribute(ns, idx["authorities"], idx[provider], TYPE_STRING, idx[provider])])
        end("provider")
    end("application")
    end("manifest")
    m.nodes.append(Node(RES_XML_END_NAMESPACE, raw=struct.pack("<HHIIIII", RES_XML_END_NAMESPACE, 16, 24, 1,
                                                             NO_INDEX, idx["android"], ns)))
    return m.build()
