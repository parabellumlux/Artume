#!/usr/bin/env python3
"""Artome Audio Web Browser Engine - Converts web pages to audio structures."""

import re
import urllib.parse
import requests
from bs4 import BeautifulSoup
import html2text

class AudioWebBrowser:
    """Headless web page parser and audio DOM navigator for Artome DE."""

    def __init__(self):
        self.current_url = None
        self.page_title = "No page loaded"
        self.headings = []
        self.links = []
        self.article_text = ""
        self.headers = {"User-Agent": "Mozilla/5.0 (X11; Linux x86_64) ArtomeAudioBrowser/1.0"}
        self.h2t = html2text.HTML2Text()
        self.h2t.ignore_links = False
        self.h2t.ignore_images = True

    def search(self, query):
        """Search DuckDuckGo HTML and return top search results for audio reading."""
        encoded_query = urllib.parse.quote(query)
        search_url = f"https://html.duckduckgo.com/html/?q={encoded_query}"
        try:
            res = requests.get(search_url, headers=self.headers, timeout=10)
            soup = BeautifulSoup(res.text, 'html.parser')
            results = []
            for a in soup.find_all('a', class_='result__a', limit=5):
                title = a.get_text().strip()
                link = a.get('href', '')
                if link.startswith('//'):
                    link = 'https:' + link
                results.append({"title": title, "url": link})

            if not results:
                return "No web results found for " + query

            summary = f"Found {len(results)} search results for {query}. "
            for idx, item in enumerate(results, 1):
                summary += f"Result {idx}: {item['title']}. "
            
            self.search_results = results
            return summary
        except Exception as e:
            return f"Search error: {str(e)[:50]}"

    def load_url(self, url):
        """Fetch and parse URL into audio DOM elements."""
        if not url.startswith("http://") and not url.startswith("https://"):
            url = "https://" + url

        try:
            res = requests.get(url, headers=self.headers, timeout=12)
            self.current_url = url
            soup = BeautifulSoup(res.text, 'html.parser')

            # Clean clutter
            for tag in soup(["script", "style", "nav", "footer", "header", "noscript"]):
                tag.decompose()

            self.page_title = soup.title.string.strip() if soup.title else "Untitled Page"

            # Extract Headings
            self.headings = []
            for h in soup.find_all(['h1', 'h2', 'h3']):
                h_text = h.get_text().strip()
                if h_text and len(h_text) > 2:
                    self.headings.append({"tag": h.name, "text": h_text})

            # Extract Links
            self.links = []
            for a in soup.find_all('a', href=True):
                link_text = a.get_text().strip()
                href = a['href']
                if link_text and len(link_text) > 2 and not href.startswith('#'):
                    full_href = urllib.parse.urljoin(url, href)
                    self.links.append({"text": link_text, "url": full_href})

            # Extract Clean Main Article Text
            main_content = soup.find('main') or soup.find('article') or soup.body
            if main_content:
                clean_html = str(main_content)
                self.article_text = self.h2t.handle(clean_html)
            else:
                self.article_text = soup.get_text()

            # Clean multiple blank lines
            self.article_text = re.sub(r'\n\s*\n', '\n\n', self.article_text).strip()

            summary = f"Loaded page: {self.page_title}. Contains {len(self.headings)} headings and {len(self.links)} links. "
            if self.article_text:
                first_paragraph = self.article_text.split('\n\n')[0][:250]
                summary += f"Preview: {first_paragraph}"
            return summary

        except Exception as e:
            return f"Failed to load web page: {str(e)[:50]}"

    def get_headings_audio(self):
        """Spoken list of headings on current page."""
        if not self.headings:
            return "No headings found on this page."
        speech = f"Page has {len(self.headings)} headings. "
        for i, h in enumerate(self.headings[:10], 1):
            speech += f"Heading {i}: {h['text']}. "
        return speech

    def get_links_audio(self, limit=7):
        """Spoken list of links on current page."""
        if not self.links:
            return "No links found on this page."
        speech = f"Top {min(limit, len(self.links))} links. "
        for i, l in enumerate(self.links[:limit], 1):
            speech += f"Link {i}: {l['text']}. "
        return speech

    def click_link_by_index(self, index):
        """Click link by 1-based audio index."""
        if 1 <= index <= len(self.links):
            target_link = self.links[index - 1]
            return self.load_url(target_link['url'])
        return f"Invalid link number {index}."

if __name__ == "__main__":
    browser = AudioWebBrowser()
    print("Testing DuckDuckGo Audio Search...")
    print(browser.search("python programming"))
