#!/usr/bin/env python3
"""Artome Screen Reader Module - AT-SPI2 Accessibility Tree Inspector & AI Summarizer."""

import os
import subprocess
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

class AtspiScreenReader:
    """AT-SPI2 Linux Accessibility Tree Inspector and Screen Summarizer."""

    def __init__(self):
        try:
            Atspi.init()
            self.available = True
        except Exception as e:
            print(f"AT-SPI2 Init Warning: {e}")
            self.available = False

    def get_active_window_title(self):
        """Fetch active window title via xdotool."""
        try:
            res = subprocess.check_output(["xdotool", "getactivewindow", "getwindowname"],
                                         stderr=subprocess.STDOUT, text=True)
            return res.strip()
        except Exception:
            return "Active Desktop Window"

    def dump_accessible_elements(self, max_elements=25):
        """Traverse AT-SPI desktop accessibility tree and extract UI elements."""
        if not self.available:
            return "AT-SPI2 accessibility daemon unavailable."

        desktop = Atspi.get_desktop(0)
        app_count = desktop.get_child_count()
        elements = []

        active_title = self.get_active_window_title()
        elements.append(f"Active Window: '{active_title}'")

        def _inspect_node(node, depth=1):
            if depth > 4 or len(elements) >= max_elements or not node:
                return

            try:
                child_count = node.get_child_count()
                for idx in range(child_count):
                    if len(elements) >= max_elements:
                        break
                    child = node.get_child_at_index(idx)
                    if not child:
                        continue

                    role_name = child.get_role_name() or "element"
                    name = child.get_name() or ""
                    description = child.get_description() or ""

                    state_set = child.get_state_set()
                    is_focused = state_set.contains(Atspi.StateType.FOCUSED) if state_set else False

                    text_val = ""
                    try:
                        text_iface = child.get_text_iface()
                        if text_iface:
                            text_val = text_iface.get_text(0, -1)
                    except Exception:
                        pass

                    info_str = f"[{role_name.upper()}] {name}".strip()
                    if is_focused:
                        info_str += " (FOCUSED)"
                    if text_val and text_val != name:
                        info_str += f" Text: '{text_val[:80]}'"
                    elif description:
                        info_str += f" Info: '{description[:80]}'"

                    if is_focused or name or text_val or role_name in ["push button", "entry", "heading", "label", "menu item", "text"]:
                        elements.append(info_str)

                    _inspect_node(child, depth + 1)
            except Exception:
                pass

        for i in range(app_count):
            app = desktop.get_child_at_index(i)
            if app:
                _inspect_node(app)

        if len(elements) == 1:
            elements.append("No accessible GUI widgets detected in tree. Window may be headless or un-instrumented.")

        return "\n".join(elements)

    def generate_screen_summary_payload(self):
        """Return formatted accessibility payload for AI screen summarization."""
        raw_tree = self.dump_accessible_elements(max_elements=30)
        return raw_tree

if __name__ == "__main__":
    sr = AtspiScreenReader()
    print("AT-SPI2 Accessibility Tree Dump:")
    print(sr.generate_screen_summary_payload())
