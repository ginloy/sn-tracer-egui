import keyboard

events = []

def print_pressed_keys(e):
    if e.name == "enter":
        strings = keyboard.get_typed_strings(events)
        for string in strings:
            if len(string) == 0:
                continue
            print(string)
        events.clear()
    if len(events) == 0:
        events.append(e)
        return
    last = events[-1]
    # print(e.time - last.time)
    if e.time - last.time < 0.015:
        events.append(e)
    else:
        events.clear()
        events.append(e)

	
keyboard.hook(print_pressed_keys)
keyboard.wait()
