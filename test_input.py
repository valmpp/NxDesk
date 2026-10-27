import sys
import time

def emulate_mouse():
    print("Testing Wayland Portal Input Injection...")
    import subprocess
    p = subprocess.Popen(['python3', 'src/NxDesk.Infrastructure/Services/remote_desktop_helper.py'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    
    # Wait for READY
    while True:
        line = p.stdout.readline()
        print("STDOUT:", line.strip())
        if "READY" in line:
            break
        if not line:
            err = p.stderr.read()
            print("STDERR:", err)
            sys.exit(1)
            
    # Send Move
    print("Sending MOVE command...")
    # X, Y coordinates
    p.stdin.write("MOVE|500|500\n")
    p.stdin.flush()
    time.sleep(1)
    
    # Send Click
    print("Sending CLICK command...")
    p.stdin.write("BUTTON|272|1\n")
    p.stdin.write("BUTTON|272|0\n")
    p.stdin.flush()
    time.sleep(1)
    
    p.kill()
    print("Test finished.")

emulate_mouse()
