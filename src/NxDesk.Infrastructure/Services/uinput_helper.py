import sys
import os
import time

# Direct /dev/uinput injector
# Bypasses Wayland Portals if /dev/uinput is accessible (root or uinput group)

try:
    import uinput
except ImportError:
    print("[uinput] Error: python-uinput module not found. Install with: pip install python-uinput", file=sys.stderr)
    sys.exit(1)

def main():
    try:
        device = uinput.Device([
            uinput.BTN_LEFT,
            uinput.BTN_RIGHT,
            uinput.BTN_MIDDLE,
            uinput.ABS_X + (0, 32767, 0, 0),
            uinput.ABS_Y + (0, 32767, 0, 0),
            uinput.REL_X,
            uinput.REL_Y,
            uinput.REL_WHEEL,
            # Add common keys
            uinput.KEY_A, uinput.KEY_B, uinput.KEY_C, uinput.KEY_D, uinput.KEY_E, uinput.KEY_F,
            uinput.KEY_G, uinput.KEY_H, uinput.KEY_I, uinput.KEY_J, uinput.KEY_K, uinput.KEY_L,
            uinput.KEY_M, uinput.KEY_N, uinput.KEY_O, uinput.KEY_P, uinput.KEY_Q, uinput.KEY_R,
            uinput.KEY_S, uinput.KEY_T, uinput.KEY_U, uinput.KEY_V, uinput.KEY_W, uinput.KEY_X,
            uinput.KEY_Y, uinput.KEY_Z, uinput.KEY_0, uinput.KEY_1, uinput.KEY_2, uinput.KEY_3,
            uinput.KEY_4, uinput.KEY_5, uinput.KEY_6, uinput.KEY_7, uinput.KEY_8, uinput.KEY_9,
            uinput.KEY_ENTER, uinput.KEY_ESC, uinput.KEY_BACKSPACE, uinput.KEY_TAB,
            uinput.KEY_LEFTSHIFT, uinput.KEY_LEFTCTRL, uinput.KEY_LEFTALT, uinput.KEY_SPACE,
            # ... can add more as needed
        ])
        print("READY")
        sys.stdout.flush()
    except Exception as e:
        print(f"[uinput] Error initializing device: {e}", file=sys.stderr)
        sys.exit(1)

    while True:
        line = sys.stdin.readline()
        if not line: break
        
        try:
            parts = line.strip().split('|')
            if not parts: continue
            cmd = parts[0]
            
            if cmd == "MOVE":
                # uinput expects ABS coordinates usually in [0, 32767]
                x = int(float(parts[1]) * 32767)
                y = int(float(parts[2]) * 32767)
                device.emit(uinput.ABS_X, x, syn=False)
                device.emit(uinput.ABS_Y, y)
            elif cmd == "BUTTON":
                btn = int(parts[1])
                down = parts[2] == "1"
                # Map standard Linux BTN codes (272=LEFT, 273=RIGHT, 274=MIDDLE)
                device.emit(btn, 1 if down else 0)
            elif cmd == "KEY":
                key = int(parts[1])
                down = parts[2] == "1"
                device.emit(key, 1 if down else 0)
            elif cmd == "SCROLL":
                dy = int(float(parts[2]))
                device.emit(uinput.REL_WHEEL, dy)
        except Exception as e:
            print(f"[uinput] Loop error: {e}", file=sys.stderr)

if __name__ == "__main__":
    main()
