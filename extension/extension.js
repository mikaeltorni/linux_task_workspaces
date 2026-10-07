import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import {placement} from './geometry.js';

const XML = `<node><interface name="org.gnome.Shell.Extensions.TaskWorkspaces">
<method name="Snapshot"><arg type="s" direction="out"/></method>
<method name="Action"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
</interface></node>`;

export default class TaskWorkspaces extends Extension {
    enable() {
        this._enabled = true;
        this._connectors = new Map();
        this._monitorManager = global.backend.get_monitor_manager();
        this._monitorSignal = this._monitorManager.connect('monitors-changed', () => this._refreshMonitors());
        this._refreshMonitors();
        this._bus = Gio.DBusExportedObject.wrapJSObject(XML, this);
        this._bus.export(Gio.DBus.session, '/org/gnome/Shell/Extensions/TaskWorkspaces');
    }

    disable() {
        this._enabled = false;
        if (this._monitorSignal)
            this._monitorManager.disconnect(this._monitorSignal);
        this._monitorSignal = null;
        this._monitorManager = null;
        this._bus?.unexport();
        this._bus = null;
    }

    _refreshMonitors() {
        Gio.DBus.session.call('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig',
            'org.gnome.Mutter.DisplayConfig', 'GetCurrentState', null, null,
            Gio.DBusCallFlags.NONE, -1, null, (connection, result) => {
                if (!this._enabled)
                    return;
                try {
                    const state = connection.call_finish(result).deep_unpack();
                    const manager = global.backend.get_monitor_manager();
                    this._connectors = new Map(state[1].map(m =>
                        [manager.get_monitor_for_connector(m[0][0]), m[0][0]]));
                } catch (e) {
                    console.error(`Task Workspaces monitor discovery: ${e.message}`);
                }
            });
    }

    windows() {
        return global.get_window_actors().map(a => a.meta_window)
            .filter(w => w.get_window_type() === Meta.WindowType.NORMAL && !w.is_skip_taskbar());
    }

    monitors() {
        return Array.from({length: global.display.get_n_monitors()}, (_, index) => {
            const g = global.display.get_monitor_geometry(index);
            return {index, connector: this._connectors.get(index) || `monitor-${index}`,
                x: g.x, y: g.y, width: g.width, height: g.height};
        });
    }

    Snapshot() {
        const monitors = this.monitors();
        const tracker = Shell.WindowTracker.get_default();
        return JSON.stringify({active: global.workspace_manager.get_active_workspace_index(),
            count: global.workspace_manager.n_workspaces, monitors,
            windows: this.windows().map(w => {
                const r = w.get_frame_rect();
                return {id: String(w.get_stable_sequence()), pid: w.get_pid(),
                    workspace: w.get_workspace().index(), sticky: w.is_on_all_workspaces(),
                    app: tracker.get_window_app(w)?.get_id() ?? '',
                    gtk_path: w.get_gtk_window_object_path() ?? '',
                    class: w.get_wm_class() ?? '', title: w.get_title() ?? '',
                    rect: [r.x, r.y, r.width, r.height],
                    monitor: monitors[w.get_monitor()]?.connector,
                    maximized: w.get_maximized(), fullscreen: w.is_fullscreen()};
            })});
    }

    Action(raw) {
        try {
            const a = JSON.parse(raw);
            if (a.op === 'ensure') {
                if (!Number.isInteger(a.workspace) || a.workspace < 0 || a.workspace > 35)
                    throw new Error('Workspace index must be 0–35 (GNOME supports 36 workspaces)');
                const count = Math.max(global.workspace_manager.n_workspaces, a.workspace + 1);
                const mutter = new Gio.Settings({schema_id: 'org.gnome.mutter'});
                const preferences = new Gio.Settings({schema_id: 'org.gnome.desktop.wm.preferences'});
                mutter.set_boolean('dynamic-workspaces', false);
                mutter.set_boolean('workspaces-only-on-primary', false);
                preferences.set_int('num-workspaces', count);
                while (global.workspace_manager.n_workspaces < count)
                    global.workspace_manager.append_new_workspace(false, global.get_current_time());
                const names = preferences.get_strv('workspace-names');
                while (names.length < count)
                    names.push('');
                if (!a.preserve_name || !names[a.workspace])
                    names[a.workspace] = a.name;
                preferences.set_strv('workspace-names', names);
                return JSON.stringify({ok: true, workspace: a.workspace});
            }
            const ws = global.workspace_manager.get_workspace_by_index(a.workspace);
            if (!ws)
                throw new Error('Workspace does not exist');
            if (a.op === 'hide' || a.op === 'close') {
                for (const w of this.windows().filter(w => a.ids.includes(String(w.get_stable_sequence())))) {
                    if (a.op === 'hide')
                        w.minimize();
                    else
                        w.delete(global.get_current_time());
                }
            } else if (a.op === 'focus') {
                ws.activate(global.get_current_time());
            } else if (a.op === 'place') {
                const w = this.windows().find(w => String(w.get_stable_sequence()) === a.id);
                if (!w)
                    throw new Error('Window no longer exists');
                const {monitor, rect} = placement(a.window, a.monitors, this.monitors());
                w.unstick();
                w.unminimize();
                w.unmake_fullscreen();
                w.unmaximize(Meta.MaximizeFlags.BOTH);
                w.change_workspace(ws);
                w.move_to_monitor(monitor.index);
                w.move_resize_frame(true, ...rect);
                if (a.window.maximized)
                    w.maximize(a.window.maximized);
                if (a.window.fullscreen)
                    w.make_fullscreen();
            } else {
                throw new Error('Unknown action');
            }
            return JSON.stringify({ok: true});
        } catch (e) {
            return JSON.stringify({error: e.message});
        }
    }
}
