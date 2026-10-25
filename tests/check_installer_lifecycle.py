"""Exercise Task Workspaces installer lifecycle without a live desktop or network."""
import json
import os
import pty
import select
import time
from pathlib import Path
import subprocess
import tempfile

def run_terminal(command, environment, keys):
    """Drive a real standalone component menu in an isolated terminal.

    Parameters: command - public installer argv; environment - isolated subprocess variables; keys - menu input bytes.
    Returns: exit status and complete terminal output.
    """
    print(f'command={command!r} environment={environment!r} keys={keys!r}')
    master, slave = pty.openpty()
    process = subprocess.Popen(command, env=environment, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    os.close(slave)
    output = bytearray()
    sent = False
    saved = False
    deadline = time.monotonic() + 20
    try:
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    break
                if not chunk:
                    break
                output.extend(chunk)
                if not sent and b'Named task workspaces' in output:
                    os.write(master, keys)
                    sent = True
                if not saved and b'Choice [1/2/3]' in output:
                    os.write(master, b'3\r')
                    saved = True
            elif process.poll() is not None:
                break
        if process.poll() is None:
            process.wait(timeout=2)
        assert sent, output.decode(errors='replace')
        result = process.returncode, output.decode(errors='replace')
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
    print(result)
    return result


ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='tws installer ') as directory:
    temporary = Path(directory)
    home = temporary / 'home'
    home.mkdir()
    tools = temporary / 'tools'
    tools.mkdir()
    calls = temporary / 'calls'
    binary = temporary / 'prebuilt-tws'
    binary.write_text('#!/bin/bash\nprintf "tws fixture\\n"\n')
    binary.chmod(0o755)
    for name in ('gsettings', 'gnome-extensions', 'systemctl', 'dpkg-query', 'curl', 'wget', 'sudo', 'cargo'):
        tool = tools / name
        tool.write_text('''#!/bin/bash
printf '%s %s\\n' "${0##*/}" "$*" >> "$TWS_TEST_CALLS"
case "${0##*/}" in
 dpkg-query) printf 'install ok installed\\n' ;;
 gsettings) printf "['task-workspaces@local']\\n" ;;
 curl|wget|sudo|cargo) exit 97 ;;
esac
''')
        tool.chmod(0o755)
    env = dict(PATH=str(tools)+':/usr/bin:/bin', HOME=str(home), USER=os.environ.get('USER', 'mk'),
               TWS_INSTALL_HOME=str(home), TWS_BINARY=str(binary), TWS_INSTALL_OFFLINE='1',
               TWS_TEST_CALLS=str(calls), XDG_CONFIG_HOME=str(home/'config'),
               XDG_STATE_HOME=str(home/'state'), XDG_CACHE_HOME=str(home/'cache'), TERM='xterm')
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--list-components'], env=env, text=True, capture_output=True)
    assert result.returncode == 0, (result.stdout, result.stderr)
    assert result.stdout.splitlines() == ['task_workspaces\tNamed task workspaces\ton'], result.stdout
    assert not calls.exists(), calls.read_text() if calls.exists() else ''
    profiles = home / '.config' / 'task-workspaces' / 'user-tasks.json'
    profiles.parent.mkdir(parents=True)
    profiles.write_text('{"important": "keep my saved workspaces"}')
    for arguments in (['--select', 'unknown'], ['--select'], ['--uninstall', 'unknown'],
                      ['--select', 'task_workspaces,task_workspaces'], ['--default', 'extra'],
                      ['--all', '--uninstall', 'task_workspaces'], ['--help', 'unexpected']):
        result = subprocess.run(['bash', str(ROOT/'install.sh'), *arguments], env=env, text=True, capture_output=True)
        assert result.returncode == 2, (arguments, result.stdout, result.stderr)
        assert not calls.exists(), (arguments, calls.read_text() if calls.exists() else '')
    # A missing prebuilt binary must reject before packages or copied files.
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--default'], env=dict(env, TWS_BINARY=str(temporary/'missing')), text=True, capture_output=True)
    assert result.returncode != 0, (result.stdout, result.stderr)
    assert not (home/'.local/bin/tws').exists()
    assert not calls.exists(), calls.read_text() if calls.exists() else ''
    # A build command failure must remain a failed component operation.
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--default'], env=dict(env, TWS_BINARY=''), text=True, capture_output=True)
    assert result.returncode != 0, (result.stdout, result.stderr)
    assert not (home/'.local/bin/tws').exists()
    calls.unlink()
    for arguments in (['--default'], ['--select', 'task_workspaces'], ['--reconfigure', 'task_workspaces'], ['--all']):
        result = subprocess.run(['bash', str(ROOT/'install.sh'), *arguments], env=env, text=True, capture_output=True)
        assert result.returncode == 0, (arguments, result.stdout, result.stderr)
        assert (home/'.local/bin/tws').read_bytes() == binary.read_bytes()
        assert (home/'.local/share/gnome-shell/extensions/task-workspaces@local/metadata.json').is_file()
        assert (home/'.config/systemd/user/task-workspaces.service').is_file()
        assert not any(token in calls.read_text() for token in ('curl ', 'wget ', 'sudo ', 'cargo ', 'systemctl ', 'gsettings ', 'gnome-extensions ')), calls.read_text()
    before = calls.read_bytes()
    for arguments in (['--detect'], ['--export-selection'], ['--help']):
        result = subprocess.run(['bash', str(ROOT/'install.sh'), *arguments], env=env, text=True, capture_output=True)
        assert result.returncode == 0, (arguments, result.stdout, result.stderr)
        if arguments == ['--detect']:
            assert result.stdout == 'task_workspaces\tinstalled\n', result.stdout
        if arguments == ['--export-selection']:
            document = json.loads(result.stdout)
            assert document['components']['task_workspaces']['on'] == 1, document
        assert calls.read_bytes() == before, arguments
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--select', 'task_workspaces'], env=dict(env, TWS_COMMAND='workspace-fixture'), text=True, capture_output=True)
    assert result.returncode == 0, (result.stdout, result.stderr)
    assert (home/'.local/bin/workspace-fixture').is_file()
    for unused in range(2):
        result = subprocess.run(['bash', str(ROOT/'install.sh'), '--uninstall', 'task_workspaces'], env=env, text=True, capture_output=True)
        assert result.returncode == 0, (result.stdout, result.stderr)
        assert not (home/'.local/bin/tws').exists()
        assert not (home/'.local/bin/workspace-fixture').exists()
        assert not (home/'.config/systemd/user/task-workspaces.service').exists()
        assert profiles.read_text() == '{"important": "keep my saved workspaces"}'
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--detect'], env=env, text=True, capture_output=True)
    assert result.returncode == 0 and result.stdout == 'task_workspaces\tabsent\n', (result.stdout, result.stderr)
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--select', ''], env=env, text=True, capture_output=True)
    assert result.returncode == 0 and not (home/'.local/bin/tws').exists(), (result.stdout, result.stderr)
    online = dict(env, TWS_INSTALL_OFFLINE='0', TWS_CONFIG_HOTKEY='none')
    result = subprocess.run(['bash', str(ROOT/'install.sh'), '--default'], env=online, text=True, capture_output=True)
    assert result.returncode == 0, (result.stdout, result.stderr)
    for unused in range(2):
        result = subprocess.run(['bash', str(ROOT/'install.sh'), '--uninstall', 'task_workspaces'], env=online, text=True, capture_output=True)
        assert result.returncode == 0, (result.stdout, result.stderr)
    assert profiles.read_text() == '{"important": "keep my saved workspaces"}'
    assert 'systemctl --user disable --now task-workspaces.service' in calls.read_text()
    before = calls.read_bytes()
    status, output = run_terminal(['bash', str(ROOT/'install.sh')], env, b'q')
    assert status == 0 and not (home/'.local/bin/tws').exists(), output
    assert calls.read_bytes() == before
    status, output = run_terminal(['bash', str(ROOT/'install.sh')], env, b'\r')
    assert status == 0 and (home/'.local/bin/tws').exists(), output
    status, output = run_terminal(['bash', str(ROOT/'install.sh')], env, b'u\r')
    assert status == 0 and not (home/'.local/bin/tws').exists(), output
    assert profiles.read_text() == '{"important": "keep my saved workspaces"}'
print('PASS: Task Workspaces selection, installation, repeatable removal, detection and preserved user profiles')
