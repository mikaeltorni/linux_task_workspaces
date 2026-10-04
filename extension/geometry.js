export function placement(saved, oldMonitors, monitors) {
    const old = oldMonitors.find(m => m.connector === saved.monitor) ?? oldMonitors[0];
    const monitor = monitors.find(m => m.connector === saved.monitor) ?? monitors[0];
    if (!old || !monitor)
        throw new Error('No monitor available');
    const [x, y, width, height] = saved.rect;
    const w = Math.min(monitor.width, Math.max(100, Math.round(width / old.width * monitor.width)));
    const h = Math.min(monitor.height, Math.max(100, Math.round(height / old.height * monitor.height)));
    return {monitor, rect: [
        Math.max(monitor.x, Math.min(monitor.x + monitor.width - w, monitor.x + Math.round((x - old.x) / old.width * monitor.width))),
        Math.max(monitor.y, Math.min(monitor.y + monitor.height - h, monitor.y + Math.round((y - old.y) / old.height * monitor.height))), w, h]};
}
