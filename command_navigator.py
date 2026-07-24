#!/usr/bin/env python3
"""Artome Audio Command Navigator & Interactive Prompt Guide Engine."""

class AudioCommandNavigator:
    """Interactive audio command menu, help navigator, and guided workflow engine for Artome OS."""

    def __init__(self):
        self.menu_categories = {
            "1": "DESKTOP",
            "2": "BROWSER",
            "3": "EMAIL",
            "4": "IDE",
            "5": "FILES",
            "6": "DOCS",
            "7": "EBOOK",
            "8": "SETTINGS"
        }

        self.mode_help = {
            "DESKTOP": (
                "Desktop Mode Commands. "
                "Say 'open firefox' to launch an app. "
                "Say 'type hello' to enter text. "
                "Say 'press enter' or 'press space' for keypresses. "
                "Say 'run command date' for shell commands. "
                "Say 'switch to browser', 'switch to email', 'switch to ide', 'switch to files', 'switch to docs', 'switch to ebook', or 'switch to settings'."
            ),
            "BROWSER": (
                "Audio Browser Mode Commands. "
                "Say 'search for python' to search DuckDuckGo. "
                "Say 'open wikipedia.org' to load a website. "
                "Say 'headings' to list page headings. "
                "Say 'links' to list page links. "
                "Say 'click link 1' to follow a link. "
                "Say 'read page' or 'pause'."
            ),
            "EMAIL": (
                "Audio Email Mode Commands. "
                "Say 'check inbox' to read unread emails. "
                "Say 'read email 1' to hear email body. "
                "Say 'compose email' for guided voice email composer. "
                "Say 'reply' to answer the last email."
            ),
            "IDE": (
                "Audio AI IDE Mode Commands. "
                "Say 'open code earcons.py' to load a script. "
                "Say 'read function play_earcon' to inspect code. "
                "Say 'read lines 1 to 20' to inspect line range. "
                "Say 'run tests' or 'explain code'."
            ),
            "FILES": (
                "Audio File Browser Mode Commands. "
                "Say 'where am I' for current directory. "
                "Say 'list folders' or 'list files' to inspect contents. "
                "Say 'read file README.md' for content preview. "
                "Say 'go to documents' or 'go up' to navigate. "
                "Say 'search file test' to locate files."
            ),
            "DOCS": (
                "Audio Document Writer Mode Commands. "
                "Say 'new document report' to start a document. "
                "Say 'add heading Summary' to add section header. "
                "Say 'add paragraph text' to dictate text. "
                "Say 'read draft' to listen to current document. "
                "Say 'export all' to generate PDF, Word, Markdown, HTML, and TXT files. "
                "Say 'share to cloud' to sync with Dropbox."
            ),
            "EBOOK": (
                "Audio EBook Reader Mode Commands. "
                "Say 'open book sample.epub' to load an ebook or PDF. "
                "Say 'list chapters' or 'table of contents'. "
                "Say 'read chapter 1' to listen. "
                "Say 'next chapter' or 'previous chapter'. "
                "Say 'set bookmark' or 'go to bookmark'. "
                "Say 'search book for keyword'."
            ),
            "SETTINGS": (
                "Audio System Settings Commands. "
                "Say 'status' for time, battery %, and WiFi network. "
                "Say 'volume up' or 'volume down'. "
                "Say 'set timer for 10 minutes'. "
                "Say 'help' to repeat this menu."
            )
        }

    def get_audio_help(self, current_mode="DESKTOP"):
        """Return spoken audio help for current active mode."""
        help_text = self.mode_help.get(current_mode.upper(), self.mode_help["DESKTOP"])
        return f"Currently in {current_mode} mode. {help_text} Say 'main menu' for full OS menu."

    def get_main_menu_audio(self):
        """Spoken audio navigation menu for all 8 Artome OS modules."""
        speech = (
            "Artome OS Main Audio Navigation Menu. "
            "Say category name or number to switch: "
            "1: Desktop, "
            "2: Web Browser, "
            "3: Email, "
            "4: Code IDE, "
            "5: File Manager, "
            "6: Document Writer, "
            "7: EBook Reader, "
            "8: Settings. "
            "Which category would you like to enter?"
        )
        return speech

    def select_menu_option(self, user_choice):
        """Parse user menu choice by number or keyword."""
        choice = user_choice.strip().lower()

        for num, mode in self.menu_categories.items():
            if choice == num or mode.lower() in choice:
                return mode, f"Switched to {mode} mode. " + self.get_audio_help(mode)

        return None, "Invalid menu choice. Say 'main menu' to list options."

if __name__ == "__main__":
    nav = AudioCommandNavigator()
    print("Testing Main Menu Audio...")
    print(nav.get_main_menu_audio())
    print("\nTesting Mode Help (IDE)...")
    print(nav.get_audio_help("IDE"))
    print("\nTesting Menu Selection ('2')...")
    print(nav.select_menu_option("2"))
