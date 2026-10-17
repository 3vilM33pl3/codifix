#!/usr/bin/python3 -I -S
"""Read-only, fixed trust-service binding observation; Python 3.11+.

Run explicitly with python3 -I -S. No install, network connection to the app,
configuration edits, service actions, recovery traversal, or secret output.
The frozen loader reference is 71cbfc9c (mem-config/src/app.rs and profile.rs).
"""
import hashlib
import ipaddress
import json
import os
import re
import selectors
import signal
import stat
import subprocess
import time
try:
    import tomllib
except ImportError:
    tomllib = None
import urllib.parse

UNIT = 'memory-layer@trust.service'
CONFIG = '/run/credentials/' + UNIT + '/memory-layer.toml'
ARGV = ['/opt/memory-layer/instances/trust/release/bin/memory', '--config', CONFIG, 'service', 'run']
EXPECTED = '477d1669e5f5fbaac81ac5be06ec2e64e10c7444ccade71b161b2d8a2d5c117e'
ENV = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C', 'LANG': 'C'}
LIMIT = 131072
DEADLINE = None


class Unknown(Exception):
    pass


def require(condition):
    if not condition:
        raise Unknown()


def read_fd(fd, limit=LIMIT):
    result = bytearray()
    while True:
        data = os.read(fd, min(8192, limit + 1 - len(result)))
        if not data:
            return bytes(result)
        result.extend(data)
        require(len(result) <= limit)


def read(path, limit=LIMIT):
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    try:
        return read_fd(fd, limit)
    finally:
        os.close(fd)


def run(argv):
    """Bound both pipes; never expose stderr or command failures."""
    with subprocess.Popen(argv, cwd='/', env=ENV, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
        try:
            deadline = time.monotonic() + 6
            if DEADLINE is not None:
                deadline = min(deadline, DEADLINE)
            output = bytearray()
            total = 0
            with selectors.DefaultSelector() as selector:
                selector.register(child.stdout, selectors.EVENT_READ)
                selector.register(child.stderr, selectors.EVENT_READ)
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    require(remaining > 0)
                    for key, _ in selector.select(min(remaining, .2)):
                        data = os.read(key.fileobj.fileno(), 8192)
                        if not data:
                            selector.unregister(key.fileobj)
                        total += len(data)
                        require(total <= LIMIT)
                        if key.fileobj is child.stdout:
                            output.extend(data)
            require(child.wait(timeout=max(.01, deadline - time.monotonic())) == 0)
            return bytes(output)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()


def service():
    raw = run(['/usr/bin/systemctl', 'show', '--no-pager',
               '--property=MainPID,ActiveState,SubState', UNIT]).decode('ascii')
    values = {}
    for row in raw.splitlines():
        key, sep, value = row.partition('=')
        require(sep and key in ('MainPID', 'ActiveState', 'SubState') and key not in values)
        values[key] = value
    require(set(values) == {'MainPID', 'ActiveState', 'SubState'})
    require(values['ActiveState'] == 'active' and values['SubState'] == 'running')
    require(re.fullmatch('[1-9][0-9]{0,9}', values['MainPID']))
    return int(values['MainPID'])


def identity(pid):
    raw = read(f'/proc/{pid}/stat', 8192)
    fields = raw[raw.rindex(b')') + 2:].split()
    require(len(fields) >= 20 and fields[19].isdigit())
    namespaces = tuple(os.stat(f'/proc/{pid}/ns/{name}').st_ino for name in ('net', 'mnt', 'pid', 'user'))
    return fields[19], namespaces


def binary_hash(pid):
    # Intentional procfs magic link: hash the opened running image, not launcher.
    fd = os.open(f'/proc/{pid}/exe', os.O_RDONLY | os.O_CLOEXEC)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and 0 < info.st_size <= 512 * 1024 * 1024)
        digest = hashlib.sha256()
        total = 0
        while True:
            data = os.read(fd, 1024 * 1024)
            if not data:
                break
            total += len(data)
            require(total <= 512 * 1024 * 1024)
            digest.update(data)
        require(total == info.st_size)
        return digest.hexdigest()
    finally:
        os.close(fd)


def trusted(info, directory=False, owners=(0,)):
    require(info.st_uid in owners and info.st_mode & 0o022 == 0)
    require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode))


def credential(pid):
    # Only this procfs root magic link is followed; each subsequent fixed
    # component uses an already-open parent and O_NOFOLLOW.
    service_uid = os.stat(f'/proc/{pid}').st_uid
    parent = os.open(f'/proc/{pid}/root', os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        trusted(os.fstat(parent), True)
        for component in CONFIG.strip('/').split('/')[:-1]:
            child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
            os.close(parent)
            parent = child
            trusted(os.fstat(parent), True, (0, service_uid) if component == UNIT else (0,))
        try:
            os.stat('memory-layer.env', dir_fd=parent, follow_symlinks=False)
        except FileNotFoundError:
            pass
        else:
            raise Unknown()
        fd = os.open('memory-layer.toml', os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
        try:
            trusted(os.fstat(fd), owners=(0, service_uid))
            return read_fd(fd, 65536)
        finally:
            os.close(fd)
    finally:
        os.close(parent)


def config_target(raw):
    config = tomllib.loads(raw.decode('utf-8'))
    url = config['database']['url']
    require(isinstance(url, str) and len(url) < 8192)
    parsed = urllib.parse.urlsplit(url)
    require(parsed.scheme in ('postgres', 'postgresql') and not parsed.query and not parsed.fragment)
    require(parsed.hostname in ('127.0.0.1', '::1') and parsed.port == 5433 and parsed.path == '/memory_trust')
    require(parsed.username is not None)
    role = urllib.parse.unquote(parsed.username, errors='strict')
    require(bool(role) and len(role.encode()) <= 63 and '\x00' not in role)
    return role, parsed.hostname


def loader_inputs(pid):
    raw = read(f'/proc/{pid}/cmdline', 8192)
    require(raw.endswith(b'\0') and raw[:-1].split(b'\0') == [v.encode() for v in ARGV])
    env = read(f'/proc/{pid}/environ', 65536)
    require(env.endswith(b'\0'))
    profile = []
    for entry in env[:-1].split(b'\0'):
        key, sep, value = entry.partition(b'=')
        require(sep and key)
        # config-rs matches environment names case-insensitively.
        require(not key.upper().startswith(b'MEMORY_LAYER__'))
        if key.upper() == b'MEMORY_LAYER_PROFILE':
            require(key == b'MEMORY_LAYER_PROFILE')
            profile.append(value)
    require(profile == [b'prod'])


def endpoint(value, ipv6):
    address, port = value.split(':')
    packed = bytes.fromhex(address)
    if ipv6:
        packed = b''.join(packed[i:i+4][::-1] for i in range(0, 16, 4))
    else:
        packed = packed[::-1]
    return ipaddress.ip_address(packed), int(port, 16)


def sockets(pid, target):
    inodes = set()
    with os.scandir(f'/proc/{pid}/fd') as entries:
        for index, entry in enumerate(entries):
            require(index < 4096)
            link = os.readlink(entry.path)
            match = re.fullmatch(r'socket:\[([0-9]+)\]', link)
            if match:
                inodes.add(int(match[1]))
    rows = set()
    for name in ('tcp', 'tcp6'):
        raw = read(f'/proc/{pid}/net/{name}', LIMIT).decode('ascii')
        for line in raw.splitlines()[1:]:
            fields = line.split()
            require(len(fields) >= 10)
            if int(fields[9]) not in inodes or fields[3] != '01':
                continue
            local, port = endpoint(fields[1], name == 'tcp6')
            remote, server_port = endpoint(fields[2], name == 'tcp6')
            if server_port == 5433 and str(remote) == target and local.is_loopback:
                rows.add((str(local), port, int(fields[9])))
    return rows


def activity():
    query = ("SELECT COALESCE(json_agg(t), '[]'::json) FROM (SELECT pg_catalog.host(client_addr) AS address, "
             "client_port AS port, usename AS role FROM pg_catalog.pg_stat_activity "
             "WHERE datname='memory_trust' AND backend_type='client backend' "
             "AND client_addr IS NOT NULL ORDER BY pid LIMIT 257) t;")
    raw = run(['/usr/sbin/runuser', '--user', 'postgres', '--', '/usr/bin/env', '-i',
               'PATH=' + ENV['PATH'], 'LC_ALL=C', 'PGPASSFILE=/dev/null',
               'PGSERVICEFILE=/dev/null', 'PGSYSCONFDIR=/nonexistent', 'PGCONNECT_TIMEOUT=2',
               'PGAPPNAME=codifix-ca-binding-check',
               'PGOPTIONS=-c default_transaction_read_only=on -c statement_timeout=4000 -c lock_timeout=1000 -c search_path=pg_catalog',
               '/usr/bin/psql', '--no-psqlrc', '--no-password', '--tuples-only', '--no-align',
               '--set=ON_ERROR_STOP=1', '--host=/run/postgresql', '--port=5433',
               '--username=postgres', '--dbname=memory_trust', '--command', query])
    rows = json.loads(raw)
    require(isinstance(rows, list) and len(rows) <= 256)
    result = set()
    for row in rows:
        require(isinstance(row, dict) and set(row) == {'address', 'port', 'role'})
        require(type(row['port']) is int and 0 < row['port'] <= 65535 and isinstance(row['role'], str))
        address = ipaddress.ip_address(row['address'])
        require(address.is_loopback)
        result.add((str(address), row['port'], row['role']))
    return result


def failure(result, stage, error):
    result['error_stage'] = stage
    result['permission_limited'] = isinstance(error, PermissionError)
    result['failure_kind'] = ('permission_limited' if result['permission_limited'] else
                              'unknown' if isinstance(error, Unknown) else 'unavailable')


def report():
    result = {'format': 'codifix-ca-binding-v1',
              'executable': 'unknown', 'configured_target': 'unknown',
              'observed_connection': 'unknown', 'process_stable': False,
              'startup_configuration_proven': False,
              'error_stage': None, 'failure_kind': None, 'permission_limited': False,
              'recovery': 'pending_operator_exact_receipt_paths_and_schema',
              'installation_or_activation_authorized': False}
    if tomllib is None:
        result.update(error_stage='python_runtime', failure_kind='unavailable')
        return result
    stage = 'service'
    try:
        pid = service()
        stage = 'process_identity'
        before = identity(pid)
        require(before[1][0] == os.stat('/proc/self/ns/net').st_ino)
        stage = 'executable'
        require(binary_hash(pid) == EXPECTED)
        result['executable'] = 'matches_pinned_rc4'
        try:
            stage = 'loader_inputs'
            loader_inputs(pid)
            stage = 'configuration'
            role, target = config_target(credential(pid))
            result['configured_target'] = 'current_process_view_loopback_5433_memory_trust'
            stage = 'connection'
            first = sockets(pid, target)
            observed = activity()
            second = sockets(pid, target)
            require(first == second and bool(first))
            require(all((address, port, role) in observed for address, port, _ in first))
            result['observed_connection'] = 'pid_socket_and_postgres_metadata_correlated'
        except Exception as error:
            failure(result, stage, error)
            result['observed_connection'] = 'unknown'
        stage = 'process_stability'
        require(service() == pid and identity(pid) == before and binary_hash(pid) == EXPECTED)
        result['process_stable'] = True
    except Exception as error:
        failure(result, stage, error)
        # Never emit exception text, parser positions, URLs, roles, environment,
        # command output, paths or records. A restart invalidates every proof.
        result['executable'] = 'unknown'
        result['configured_target'] = 'unknown'
        result['observed_connection'] = 'unknown'
    return result


def main():
    global DEADLINE
    DEADLINE = time.monotonic() + 30
    signal.signal(signal.SIGALRM, lambda *_: (_ for _ in ()).throw(Unknown()))
    signal.alarm(30)
    result = report()
    signal.alarm(0)
    print(json.dumps(result, sort_keys=True))
    return 0 if result['observed_connection'] != 'unknown' else 1


if __name__ == '__main__':
    raise SystemExit(main())
