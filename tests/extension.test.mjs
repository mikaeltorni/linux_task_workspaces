// Run: node --test tests/*.test.mjs
// Coverage: GNOME bridge snapshot fields, permanent slot/settings preservation,
// geometry restoration, hide/close only selected windows, invalid workspace ->
// cases below. Mock GNOME dependencies; never manipulate the real desktop.
// Lifecycle regression: MonitorManager owns monitors-changed, not Display;
// disconnect/unexport on disable and ignore pending discovery callbacks.
// GNOME accepts at most 36 static workspaces; reject slot 36 before mutation.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {placement} from '../extension/geometry.js';

function fixture() {
    const calls = [];
    const settings = new Map([['workspace-names', ['Main', 'Code']], ['num-workspaces', 8]]);
    const window = {
        get_stable_sequence: () => 42, get_pid: () => 100, get_workspace: () => ({index: () => 2}),
        get_window_type: () => 0, is_skip_taskbar: () => false, is_on_all_workspaces: () => false,
        get_wm_class: () => 'kitty', get_title: () => 'Backup', get_monitor: () => 0,
        get_gtk_window_object_path: () => '',
        get_frame_rect: () => ({x: 0, y: -1080, width: 960, height: 1080}),
        get_maximized: () => 0, is_fullscreen: () => false,
    };
    for (const method of ['unstick', 'unminimize', 'unmake_fullscreen', 'unmaximize', 'change_workspace',
        'move_to_monitor', 'move_resize_frame', 'maximize', 'make_fullscreen', 'minimize', 'delete'])
        window[method] = (...args) => calls.push([method, ...args]);
    const manager = {n_workspaces: 8, get_active_workspace_index: () => 2,
        get_workspace_by_index: i => i < manager.n_workspaces ? {activate: () => calls.push(['focus', i])} : null,
        append_new_workspace: () => manager.n_workspaces++};
    const monitorManager = {
        connect: (signal, callback) => {
            assert.equal(signal, 'monitors-changed');
            calls.push(['monitor-connect', callback]);
            return 51;
        },
        disconnect: id => calls.push(['monitor-disconnect', id]),
        get_monitor_for_connector: () => 0,
    };
    const pending = [];
    const context = vm.createContext({console, placement, Extension: class {},
        Meta: {WindowType: {NORMAL: 0}, MaximizeFlags: {BOTH: 3}},
        Shell: {WindowTracker: {get_default: () => ({get_window_app: () => ({get_id: () => 'kitty.desktop'})})}},
        Gio: {DBusCallFlags: {NONE: 0}, DBus: {session: {
            call: (...args) => pending.push(args.at(-1)),
        }}, DBusExportedObject: {wrapJSObject: () => ({
            export: () => calls.push(['export']), unexport: () => calls.push(['unexport']),
        })}, Settings: class {
            set_boolean(k, v) {settings.set(k, v);}
            set_int(k, v) {settings.set(k, v);}
            set_strv(k, v) {settings.set(k, v);}
            get_strv(k) {return [...settings.get(k)];}
        }},
        global: {workspace_manager: manager, get_current_time: () => 1,
            backend: {get_monitor_manager: () => monitorManager},
            get_window_actors: () => [{meta_window: window}],
            display: {get_n_monitors: () => 1,
                get_monitor_geometry: () => ({x: 0, y: -1080, width: 1920, height: 1080})}},
    });
    const source = fs.readFileSync(new URL('../extension/extension.js', import.meta.url), 'utf8')
        .replace(/^import .*;\n/gm, '').replace('export default class', 'class');
    vm.runInContext(source + '\nglobalThis.instance = new TaskWorkspaces();', context);
    context.instance._connectors = new Map([[0, 'DP-1']]);
    return {...context, calls, settings, pending};
}

test('enable watches MonitorManager and disable releases the bridge', () => {
    const f = fixture();
    f.instance.enable();
    assert.equal(f.calls[0][0], 'monitor-connect');
    assert.ok(f.calls.some(c => c[0] === 'export'));
    f.instance.disable();
    assert.ok(f.calls.some(c => c[0] === 'monitor-disconnect' && c[1] === 51));
    assert.ok(f.calls.some(c => c[0] === 'unexport'));
    f.pending[0]({call_finish: () => {throw new Error('late callback must be ignored');}}, {});
    assert.equal(f.instance._connectors.size, 0);
});

test('snapshot remembers each window and its named monitor', () => {
    const f = fixture();
    const s = JSON.parse(f.instance.Snapshot());
    assert.equal(s.windows[0].monitor, 'DP-1');
    assert.deepEqual(s.windows[0].rect, [0, -1080, 960, 1080]);
    assert.equal(s.windows[0].app, 'kitty.desktop');
});
test('reserves permanent appended slot without overwriting existing names', () => {
    const f = fixture();
    assert.deepEqual(JSON.parse(f.instance.Action(JSON.stringify({op: 'ensure', workspace: 8, name: 'Task: backup'}))), {ok: true, workspace: 8});
    assert.equal(f.settings.get('num-workspaces'), 9);
    assert.equal(f.settings.get('dynamic-workspaces'), false);
    assert.deepEqual(f.settings.get('workspace-names').slice(0, 2), ['Main', 'Code']);
    assert.equal(f.settings.get('workspace-names')[8], 'Task: backup');
    assert.ok(JSON.parse(f.instance.Action(JSON.stringify({op: 'ensure', workspace: -1}))).error);
    assert.equal(f.settings.get('num-workspaces'), 9);
    assert.ok(JSON.parse(f.instance.Action(JSON.stringify({op: 'ensure', workspace: 36}))).error);
    assert.equal(f.settings.get('num-workspaces'), 9);
});
test('places and hides only selected saved windows', () => {
    const f = fixture();
    const snapshot = JSON.parse(f.instance.Snapshot());
    const saved = snapshot.windows[0];
    f.instance.Action(JSON.stringify({op: 'place', workspace: 2, id: '42', window: saved, monitors: snapshot.monitors}));
    assert.deepEqual(f.calls.find(c => c[0] === 'move_resize_frame'), ['move_resize_frame', true, 0, -1080, 960, 1080]);
    f.instance.Action(JSON.stringify({op: 'hide', workspace: 2, ids: ['other']}));
    assert.ok(!f.calls.some(c => c[0] === 'minimize'));
    f.instance.Action(JSON.stringify({op: 'hide', workspace: 2, ids: ['42']}));
    assert.ok(f.calls.some(c => c[0] === 'minimize'));
    f.instance.Action(JSON.stringify({op: 'close', workspace: 2, ids: ['42']}));
    assert.ok(f.calls.some(c => c[0] === 'delete'));
});
