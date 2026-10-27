import sys
import gi
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib
import random
import argparse

# Unified Portal Helper for Wayland
# Creates a RemoteDesktop session, adds ScreenCast sources to it,
# then calls Start on RemoteDesktop which returns streams AND enables input.
# Prints PipeWire Node IDs to stdout, accepts input commands from stdin.

class UnifiedPortalHelper:
    def __init__(self, restore_token=None):
        self.restore_token = restore_token
        self.loop = GLib.MainLoop()
        self.session_handle = None
        self._step = 0  # 0=waiting session, 1=waiting devices, 2=waiting sources, 3=waiting start
        
        try:
            self.connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            print("[Portal] D-Bus connection established", file=sys.stderr)
            
            self.sc_proxy = Gio.DBusProxy.new_sync(
                self.connection, Gio.DBusProxyFlags.NONE, None,
                'org.freedesktop.portal.Desktop', '/org/freedesktop/portal/desktop',
                'org.freedesktop.portal.ScreenCast', None
            )
            self.rd_proxy = Gio.DBusProxy.new_sync(
                self.connection, Gio.DBusProxyFlags.NONE, None,
                'org.freedesktop.portal.Desktop', '/org/freedesktop/portal/desktop',
                'org.freedesktop.portal.RemoteDesktop', None
            )
        except Exception as e:
            print(f"[Portal Error] D-Bus init failed: {e}", file=sys.stderr)
            sys.exit(1)

    def run(self):
        self.connection.signal_subscribe(
            'org.freedesktop.portal.Desktop', 'org.freedesktop.portal.Request',
            'Response', None, None, Gio.DBusSignalFlags.NONE,
            self.on_response, None
        )
        self._do_create_session()
        self.loop.run()

    # ── Step 0: Create session via RemoteDesktop ──
    def _do_create_session(self):
        token = "nxdesk_session"
        options = {
            'session_handle_token': GLib.Variant('s', token),
            'handle_token': GLib.Variant('s', token),
        }
        self._step = 0
        try:
            print(f"[Portal] Step 0: CreateSession (RemoteDesktop) restore={self.restore_token}...", file=sys.stderr)
            self.rd_proxy.call_sync(
                "CreateSession", GLib.Variant('(a{sv})', (options,)),
                Gio.DBusCallFlags.NONE, 10000, None
            )
            print("[Portal] CreateSession CALL sent", file=sys.stderr)
        except Exception as e:
            print(f"[Portal Error] CreateSession failed: {e}", file=sys.stderr)
            self.loop.quit()

    # ── Step 1: SelectDevices via RemoteDesktop ──
    def _do_select_devices(self):
        token = "nxdesk_dev"
        options = {
            'handle_token': GLib.Variant('s', token),
            'types': GLib.Variant('u', 3),  # keyboard=1 | pointer=2
            'persistance_mode': GLib.Variant('u', 2), # Always persist
        }
        if self.restore_token:
            options['restore_token'] = GLib.Variant('s', self.restore_token)

        self._step = 1
        try:
            print("[Portal] Step 1: SelectDevices (RemoteDesktop)...", file=sys.stderr)
            self.rd_proxy.call_sync(
                "SelectDevices", GLib.Variant('(oa{sv})', (self.session_handle, options)),
                Gio.DBusCallFlags.NONE, 5000, None
            )
        except Exception as e:
            print(f"[Portal Error] SelectDevices failed: {e}", file=sys.stderr)
            self.loop.quit()

    # ── Step 2: SelectSources via ScreenCast ──
    def _do_select_sources(self):
        token = "nxdesk_src"
        options = {
            'handle_token': GLib.Variant('s', token),
            'types': GLib.Variant('u', 1),       # monitors
            'cursor_mode': GLib.Variant('u', 2),  # embedded
            'persistance_mode': GLib.Variant('u', 2), # Always persist
        }
        if self.restore_token:
            options['restore_token'] = GLib.Variant('s', self.restore_token)

        self._step = 2
        try:
            print("[Portal] Step 2: SelectSources (ScreenCast)...", file=sys.stderr)
            self.sc_proxy.call_sync(
                "SelectSources", GLib.Variant('(oa{sv})', (self.session_handle, options)),
                Gio.DBusCallFlags.NONE, 5000, None
            )
        except Exception as e:
            print(f"[Portal Error] SelectSources failed: {e}", file=sys.stderr)
            self.loop.quit()

    # ── Step 3: Start via RemoteDesktop (NOT ScreenCast!) ──
    # This is the critical fix: RemoteDesktop.Start returns the streams
    # and activates input in a single call.
    def _do_start(self):
        token = "nxdesk_start"
        self._step = 3
        try:
            print("[Portal] Step 3: Start (RemoteDesktop) — unified start...", file=sys.stderr)
            self.rd_proxy.call_sync(
                "Start", GLib.Variant('(osa{sv})', (self.session_handle, "", {'handle_token': GLib.Variant('s', token)})),
                Gio.DBusCallFlags.NONE, 30000, None   # 30s for user dialog
            )
        except Exception as e:
            print(f"[Portal Error] Start failed: {e}", file=sys.stderr)
            self.loop.quit()

    # ── Central response handler ──
    def on_response(self, conn, sender, path, iface, signal, params, user_data):
        response, details = params.unpack()
        keys = list(details.keys()) if details else []
        print(f"[Portal Signal] step={self._step} response={response} keys={keys}", file=sys.stderr)

        if response != 0:
            print(f"[Portal Error] Non-zero response {response} at step {self._step}. Details: {details}", file=sys.stderr)
            self.loop.quit()
            return

        if self._step == 0:
            # CreateSession response → extract session_handle
            if "session_handle" in details:
                self.session_handle = details['session_handle']
                print(f"[Portal] Session: {self.session_handle}", file=sys.stderr)
                self._do_select_devices()
            else:
                print("[Portal Error] No session_handle in CreateSession response", file=sys.stderr)
                self.loop.quit()
            return

        if self._step == 1:
            # SelectDevices response
            self._do_select_sources()
            return

        if self._step == 2:
            # SelectSources response
            self._do_start()
            return

        if self._step == 3:
            # Start response — contains 'streams' with PipeWire node IDs
            if "streams" in details:
                for stream in details['streams']:
                    node_id = stream[0]
                    props = stream[1] if len(stream) > 1 else {}
                    sz = props.get('size')
                    if sz:
                        w, h = sz
                        print(f"NODE_ID:{node_id},SIZE:{w}x{h}")
                    else:
                        print(f"NODE_ID:{node_id}")

                if "restore_token" in details:
                    print(f"RESTORE_TOKEN:{details['restore_token']}")

                print("READY")
                sys.stdout.flush()
                print("[Portal] ✓ Streams ready, input enabled. Listening on stdin.", file=sys.stderr)
                GLib.io_add_watch(sys.stdin, GLib.IO_IN, self.on_stdin_data)
            else:
                print(f"[Portal Error] Start response has no streams: {keys}", file=sys.stderr)
                self.loop.quit()
            return

    # ── Input command handler ──
    def on_stdin_data(self, source, condition):
        line = sys.stdin.readline()
        if not line:
            print("[Portal] Stdin closed, exiting.", file=sys.stderr)
            self.loop.quit()
            return False
        
        try:
            parts = line.strip().split('|')
            if not parts or not parts[0]: return True
            cmd = parts[0]
            if cmd == "MOVE":
                x, y = float(parts[1]), float(parts[2])
                # print(f"[Portal] Inject MOVE: {x}, {y}", file=sys.stderr)
                self.rd_proxy.call(
                    "NotifyPointerMotionAbsolute",
                    GLib.Variant('(oa{sv}udd)', (self.session_handle, {}, 0, x, y)),
                    Gio.DBusCallFlags.NONE, 1000, None, None, None
                )
            elif cmd == "BUTTON":
                btn, down = int(parts[1]), parts[2] == "1"
                # print(f"[Portal] Inject BUTTON: {btn} {'DOWN' if down else 'UP'}", file=sys.stderr)
                self.rd_proxy.call(
                    "NotifyPointerButton",
                    GLib.Variant('(oa{sv}iu)', (self.session_handle, {}, btn, 1 if down else 0)),
                    Gio.DBusCallFlags.NONE, 1000, None, None, None
                )
            elif cmd == "KEY":
                key, down = int(parts[1]), parts[2] == "1"
                # print(f"[Portal] Inject KEY: {key} {'DOWN' if down else 'UP'}", file=sys.stderr)
                self.rd_proxy.call(
                    "NotifyKeyboardKey",
                    GLib.Variant('(oa{sv}iu)', (self.session_handle, {}, key, 1 if down else 0)),
                    Gio.DBusCallFlags.NONE, 1000, None, None, None
                )
            elif cmd == "SCROLL":
                dx, dy = float(parts[1]), float(parts[2])
                if dy != 0:
                    self.rd_proxy.call(
                        "NotifyPointerAxis",
                        GLib.Variant('(oa{sv}ud)', (self.session_handle, {}, 0, dy)),
                        Gio.DBusCallFlags.NONE, 1000, None, None, None
                    )
                if dx != 0:
                    self.rd_proxy.call(
                        "NotifyPointerAxis",
                        GLib.Variant('(oa{sv}ud)', (self.session_handle, {}, 1, dx)),
                        Gio.DBusCallFlags.NONE, 1000, None, None, None
                    )
        except Exception as e:
            print(f"[Input Error] {e}", file=sys.stderr)
        return True

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--restore-token", help="Previous restore token")
    args = parser.parse_args()
    helper = UnifiedPortalHelper(restore_token=args.restore_token)
    helper.run()
