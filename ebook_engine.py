#!/usr/bin/env python3
"""Artome Audio EBook & Document Reader - EPUB, PDF & Text voice navigation engine."""

import os
import re
import ebooklib
from ebooklib import epub
from pypdf import PdfReader
from bs4 import BeautifulSoup
import html2text

class AudioEBookReader:
    """Voice-first EPUB and PDF reader for Artome DE."""

    def __init__(self):
        self.active_book_path = None
        self.book_title = "No book loaded"
        self.author = "Unknown Author"
        self.chapters = []  # List of {"title": str, "content": str}
        self.current_chapter_idx = 0
        self.bookmark_idx = 0
        self.h2t = html2text.HTML2Text()
        self.h2t.ignore_links = True
        self.h2t.ignore_images = True

    def load_book(self, filepath):
        """Load an EPUB, PDF, or TXT file into audio chapter tree."""
        if not os.path.isabs(filepath):
            filepath = os.path.abspath(filepath)

        if not os.path.exists(filepath):
            return f"Book file not found: {filepath}"

        self.active_book_path = filepath
        self.chapters = []
        self.current_chapter_idx = 0

        ext = os.path.splitext(filepath)[1].lower()

        try:
            if ext == ".epub":
                return self._load_epub(filepath)
            elif ext == ".pdf":
                return self._load_pdf(filepath)
            elif ext in [".txt", ".md"]:
                return self._load_txt(filepath)
            else:
                return f"Unsupported ebook format '{ext}'. Supported formats: .epub, .pdf, .txt, .md."
        except Exception as e:
            return f"Failed to load ebook: {str(e)[:50]}"

    def _load_epub(self, filepath):
        book = epub.read_epub(filepath)
        self.book_title = book.get_metadata('DC', 'title')
        self.book_title = self.book_title[0][0] if self.book_title else os.path.basename(filepath)

        authors = book.get_metadata('DC', 'creator')
        self.author = authors[0][0] if authors else "Unknown Author"

        for item in book.get_items_of_type(ebooklib.ITEM_DOCUMENT):
            soup = BeautifulSoup(item.get_content(), 'html.parser')
            heading = soup.find(['h1', 'h2', 'h3'])
            ch_title = heading.get_text().strip() if heading else f"Chapter {len(self.chapters) + 1}"
            
            clean_text = self.h2t.handle(str(soup)).strip()
            if len(clean_text) > 50:
                self.chapters.append({"title": ch_title, "content": clean_text})

        return f"Loaded EPUB book '{self.book_title}' by {self.author}. Contains {len(self.chapters)} chapters. Say 'read chapter 1' or 'list chapters'."

    def _load_pdf(self, filepath):
        reader = PdfReader(filepath)
        self.book_title = os.path.basename(filepath)
        self.author = "PDF Document"

        total_pages = len(reader.pages)
        # Group pages into chapters (every 5 pages or per page)
        for i, page in enumerate(reader.pages, 1):
            text = page.extract_text() or ""
            if text.strip():
                self.chapters.append({"title": f"Page {i}", "content": text.strip()})

        return f"Loaded PDF document '{self.book_title}'. Contains {len(self.chapters)} pages. Say 'read chapter 1'."

    def _load_txt(self, filepath):
        self.book_title = os.path.basename(filepath)
        self.author = "Text Document"
        with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()

        # Split into pseudo-chapters by double newlines or 1000 character chunks
        chunks = [c.strip() for c in content.split("\n\n") if len(c.strip()) > 30]
        for idx, chunk in enumerate(chunks, 1):
            self.chapters.append({"title": f"Section {idx}", "content": chunk})

        return f"Loaded text file '{self.book_title}'. Contains {len(self.chapters)} sections."

    def list_chapters_audio(self, limit=8):
        """Spoken list of book chapters."""
        if not self.chapters:
            return "No book currently loaded."
        speech = f"Book '{self.book_title}' has {len(self.chapters)} chapters. "
        for idx, ch in enumerate(self.chapters[:limit], 1):
            speech += f"Chapter {idx}: {ch['title']}. "
        return speech

    def read_chapter_audio(self, chapter_num=1):
        """Read a specific chapter out loud."""
        if not self.chapters:
            return "No book currently loaded."
        if 1 <= chapter_num <= len(self.chapters):
            self.current_chapter_idx = chapter_num - 1
            ch = self.chapters[self.current_chapter_idx]
            preview = ch['content'][:600].replace('\n', ' ')
            return f"Reading Chapter {chapter_num}, {ch['title']}: {preview}"
        return f"Invalid chapter number {chapter_num}. Book has {len(self.chapters)} chapters."

    def next_chapter_audio(self):
        """Advance to next chapter."""
        if self.current_chapter_idx + 1 < len(self.chapters):
            return self.read_chapter_audio(self.current_chapter_idx + 2)
        return "You have reached the end of the book."

    def previous_chapter_audio(self):
        """Go back to previous chapter."""
        if self.current_chapter_idx > 0:
            return self.read_chapter_audio(self.current_chapter_idx)
        return "You are at the first chapter."

    def set_bookmark(self):
        """Bookmark current chapter position."""
        self.bookmark_idx = self.current_chapter_idx
        return f"Set bookmark at chapter {self.bookmark_idx + 1}."

    def read_bookmark(self):
        """Return to bookmarked position."""
        return self.read_chapter_audio(self.bookmark_idx + 1)

    def search_book_audio(self, query):
        """Search text content across all chapters."""
        if not self.chapters:
            return "No book currently loaded."
        matches = []
        for idx, ch in enumerate(self.chapters, 1):
            if query.lower() in ch['content'].lower():
                matches.append(f"Chapter {idx} ({ch['title']})")
            if len(matches) >= 4:
                break

        if not matches:
            return f"No matches found for '{query}' in {self.book_title}."

        return f"Found '{query}' in {len(matches)} chapters: {', '.join(matches)}."

if __name__ == "__main__":
    reader = AudioEBookReader()
    print("Testing Audio EBook Reader...")
    # Create sample txt book for verification
    sample_path = "/tmp/sample_book.txt"
    with open(sample_path, "w") as f:
        f.write("Chapter 1: The Beginning\n\nOnce upon a time in the world of Artome OS, an audio-first desktop environment was born.\n\nChapter 2: The Journey\n\nBlind users could navigate web pages, write documents, and read ebooks effortlessly with natural voice commands.")
    
    print(reader.load_book(sample_path))
    print(reader.list_chapters_audio())
    print(reader.read_chapter_audio(1))
    print(reader.search_book_audio("Artome"))
