"""Private create-only native onboarding files; never manage legacy settings."""
import fcntl
import ctypes
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import uuid


class Refused(Exception):
    pass


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def path(value):
    result = Path(value)
    if not result.is_absolute() or ".." in result.parts or len(str(result)) > 4096 or any(ord(c) < 32 for c in str(result)):
        raise Refused("bounded absolute paths without traversal are required")
    for ancestor in [*reversed(result.parents), result]:
        if ancestor.is_symlink():
            raise Refused("symlinked operator/data paths are unsupported; existing paths preserved")
    return result


def private(file):
    file = path(file)
    info = file.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077 or info.st_nlink != 1:
        raise Refused("metadata/config must be a current-user private regular file (0600 or stricter)")
    return file


def parent_descriptor(file):
    """Pin directory descriptors: don't validate one lookup then use another."""
    file = path(file)
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for component in file.parts[1:-1]:
            info = os.fstat(fd)
            if info.st_uid not in {0, os.getuid()} or (info.st_mode & 0o022 and not info.st_mode & stat.S_ISVTX):
                raise Refused("untrusted mutable path ancestor")
            next_fd = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        info = os.fstat(fd)
        if info.st_uid not in {0, os.getuid()} or (info.st_mode & 0o022 and not info.st_mode & stat.S_ISVTX):
            raise Refused("untrusted mutable path parent")
        result, fd = fd, None
        return result
    finally:
        if fd is not None:
            os.close(fd)


def opened(file, *, protected):
    file = path(file)
    fd = parent_descriptor(file)
    try:
        stream_fd = os.open(file.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
        info = os.fstat(stream_fd)
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or (protected and (info.st_uid != os.getuid() or info.st_mode & 0o077)) or (not protected and info.st_mode & 0o022):
            os.close(stream_fd)
            raise Refused("unsafe private/prerequisite file")
        return os.fdopen(stream_fd, "rb")
    finally:
        os.close(fd)


def bytes_at(file, limit=1024 * 1024):
    with opened(file, protected=True) as stream:
        value = stream.read(limit + 1)
    if len(value) > limit:
        raise Refused("metadata exceeds safe bounds")
    return value


def read(file, limit=1024 * 1024):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise Refused("duplicate metadata keys")
            result[key] = value
        return result
    return json.loads(bytes_at(file, limit), object_pairs_hook=unique)


def sha(file):
    file = path(file)
    info = file.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > 1024 * 1024 * 1024 or info.st_mode & 0o022:
        raise Refused("prerequisite must be a bounded regular file, not group/world-writable")
    with opened(file, protected=False) as stream:
        result, total = hashlib.sha256(), 0
        while chunk := stream.read(1024 * 1024):
            total += len(chunk)
            if total > 1024 * 1024 * 1024:
                raise Refused("prerequisite exceeds bounded SHA-256 inspection")
            result.update(chunk)
        return result.hexdigest()


def outside(a, b):
    a, b = path(a), path(b)
    def identity(p):
        info = p.stat()
        return info.st_dev, info.st_ino
    def inside(child, root):
        if not root.exists():
            return False
        while not child.exists():
            child = child.parent
        child = child.resolve(strict=True)
        return any(identity(p) == identity(root) for p in [child, *child.parents])
    if inside(a, b) or inside(b, a):
        raise Refused("journal/data/lock paths must be physically separate")
    if not a.exists() and not b.exists() and identity(a.parent) == identity(b.parent):
        if not a.name.isascii() or not b.name.isascii() or a.name.lower() == b.name.lower():
            raise Refused("ambiguous new filesystem aliases; use distinct ASCII names")


def directory(file):
    file = path(file)
    if file.exists():
        info = file.stat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise Refused("operator directory is not current-user private; no ownership/permission changes")
    else:
        file.mkdir(mode=0o700)
    return file


def missing(file, data):
    file = path(file)
    if file.exists():
        if bytes_at(file) != data:
            raise Refused("existing operator file differs; preserve it and inspect, never overwrite")
        return
    temporary = ".codifix-new-" + uuid.uuid4().hex
    parent = parent_descriptor(file)
    try:
        fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        # Linking then unlinking has a crash window leaving nlink=2. Protected
        # readers correctly reject that file, which made exact recovery fail.
        # Publish once with the platform's atomic no-replace rename instead.
        rename_missing(parent, temporary, file.name)
        os.fsync(parent)
    finally:
        try:
            os.unlink(temporary, dir_fd=parent)
        except FileNotFoundError:
            pass
        os.close(parent)


def rename_missing(parent, source, target):
    if sys.platform == "darwin":
        symbol, flags = "renameatx_np", 0x00000004  # RENAME_EXCL
    elif sys.platform == "linux":
        symbol, flags = "renameat2", 1  # RENAME_NOREPLACE; offline fixture host
    else:
        raise Refused("atomic create-only publication requires Darwin or Linux")
    libc = ctypes.CDLL(None, use_errno=True)
    try:
        rename = getattr(libc, symbol)
    except AttributeError:
        raise Refused("atomic create-only rename unavailable; no unsafe fallback") from None
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(parent, os.fsencode(source), parent, os.fsencode(target), flags) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), target)


def save(file, journal):
    file = private(file)
    previous = read(file)
    if previous["request_id"] != journal["request_id"] or previous["plan_sha256"] != journal["plan_sha256"]:
        raise Refused("journal changed; no replacement")
    temporary = ".codifix-journal-" + uuid.uuid4().hex
    parent = parent_descriptor(file)
    try:
        fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        with os.fdopen(fd, "wb") as stream:
            stream.write(encoded(journal))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, file.name, src_dir_fd=parent, dst_dir_fd=parent)
        os.fsync(parent)
    finally:
        try:
            os.unlink(temporary, dir_fd=parent)
        except FileNotFoundError:
            pass
        os.close(parent)


class Lock:
    def __init__(self, file):
        self.file = path(str(file) + ".codifix-lock")
    def __enter__(self):
        parent = parent_descriptor(self.file)
        try:
            fd = os.open(self.file.name, os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        finally:
            os.close(parent)
        self.stream = os.fdopen(fd, "wb")
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077 or info.st_nlink != 1:
                raise Refused("unsafe lock file")
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except Exception:
            self.stream.close()
            raise Refused("operator journal is locked or unsafe; no native effects") from None
        return self
    def __exit__(self, *_):
        self.stream.close()
