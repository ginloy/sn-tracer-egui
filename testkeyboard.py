import sys
sys.path.append('..')
import keyboard

def print_pressed_keys(e):
    if e.event_type == "up":
        return
    if e.name == "enter":
        print("\r", end="", flush=True)
    elif e.name == "space":
        print(" ", end="", flush=True)
    elif len(e.name) == 1:
        print(e.name, end="", flush=True)

	
keyboard.hook(print_pressed_keys)
keyboard.wait()
