#!/usr/bin/env python3
"""
Convert mdBook documentation to GitHub Wiki format.

GitHub Wiki has a flat structure with files named using the page title.
Links between pages use [[Page Name]] or [text](Page-Name) format.
"""

import os
import re
import shutil
from pathlib import Path

BOOK_SRC = Path("docs/book/src")
WIKI_DIR = Path("wiki")

# Mapping from file paths to wiki page names
PAGE_MAP = {}

def slugify(name: str) -> str:
    """Convert a name to a wiki-friendly slug."""
    # Replace spaces and special chars with hyphens
    slug = re.sub(r'[^\w\s-]', '', name)
    slug = re.sub(r'[\s_]+', '-', slug)
    return slug.strip('-')

def parse_summary() -> list:
    """Parse SUMMARY.md to get the chapter order and structure."""
    summary_path = BOOK_SRC / "SUMMARY.md"
    chapters = []

    with open(summary_path, 'r', encoding='utf-8') as f:
        content = f.read()

    # Match markdown links: [Title](path.md)
    pattern = r'\[([^\]]+)\]\(([^)]+\.md)\)'

    for match in re.finditer(pattern, content):
        title = match.group(1)
        path = match.group(2)

        # Skip external links
        if path.startswith('http'):
            continue

        chapters.append({
            'title': title,
            'path': path,
            'wiki_name': slugify(title)
        })

    return chapters

def build_page_map(chapters: list) -> dict:
    """Build a mapping from file paths to wiki page names."""
    page_map = {}

    for chapter in chapters:
        # Normalize the path (remove ./ prefix if present)
        path = chapter['path'].lstrip('./')
        page_map[path] = chapter['wiki_name']

        # Also map without .md extension
        path_no_ext = path.rsplit('.md', 1)[0]
        page_map[path_no_ext] = chapter['wiki_name']
        page_map[path_no_ext + '.md'] = chapter['wiki_name']

    return page_map

def convert_links(content: str, current_file: Path, page_map: dict) -> str:
    """Convert mdBook-style links to GitHub Wiki links."""

    def replace_link(match):
        text = match.group(1)
        href = match.group(2)

        # Skip external links and anchors
        if href.startswith('http') or href.startswith('#'):
            return match.group(0)

        # Handle relative paths
        if href.startswith('./') or href.startswith('../'):
            # Resolve the path relative to current file
            current_dir = current_file.parent
            resolved = (current_dir / href).resolve()

            try:
                # Make it relative to BOOK_SRC
                rel_path = resolved.relative_to(BOOK_SRC.resolve())
                href = str(rel_path)
            except ValueError:
                pass

        # Remove .md extension for lookup
        href_clean = href.rsplit('.md', 1)[0] + '.md'
        href_clean = href_clean.lstrip('./')

        # Look up in page map
        if href_clean in page_map:
            wiki_page = page_map[href_clean]
            return f'[{text}]({wiki_page})'

        # Try without .md
        href_no_ext = href.rsplit('.md', 1)[0].lstrip('./')
        if href_no_ext in page_map:
            wiki_page = page_map[href_no_ext]
            return f'[{text}]({wiki_page})'

        # Keep as-is if not found
        return match.group(0)

    # Match markdown links: [text](path)
    pattern = r'\[([^\]]+)\]\(([^)]+)\)'
    return re.sub(pattern, replace_link, content)

def process_content(content: str, current_file: Path, page_map: dict) -> str:
    """Process markdown content for wiki compatibility."""

    # Convert links
    content = convert_links(content, current_file, page_map)

    # Remove mdBook-specific markers
    content = re.sub(r'<!--\s*toc\s*-->', '', content, flags=re.IGNORECASE)

    return content

def create_wiki_pages(chapters: list, page_map: dict):
    """Create wiki pages from book chapters."""

    for chapter in chapters:
        src_path = BOOK_SRC / chapter['path']
        wiki_name = chapter['wiki_name']
        wiki_path = WIKI_DIR / f"{wiki_name}.md"

        if not src_path.exists():
            print(f"Warning: {src_path} not found, skipping")
            continue

        with open(src_path, 'r', encoding='utf-8') as f:
            content = f.read()

        # Process content
        content = process_content(content, src_path, page_map)

        # Write to wiki
        with open(wiki_path, 'w', encoding='utf-8') as f:
            f.write(content)

        print(f"Created: {wiki_path}")

def create_home_page(chapters: list, page_map: dict):
    """Create the Home.md wiki page with navigation."""

    content = """# Blinc UI Framework

A GPU-accelerated, reactive UI framework for Rust.

## Documentation

"""

    # Read SUMMARY.md to get section headers
    summary_path = BOOK_SRC / "SUMMARY.md"
    with open(summary_path, 'r', encoding='utf-8') as f:
        summary_content = f.read()

    lines = summary_content.split('\n')

    for line in lines:
        # Check for section headers (# Header)
        section_match = re.match(r'^#\s+(.+)$', line)
        if section_match:
            section_name = section_match.group(1)
            if section_name != "Summary":
                content += f"\n### {section_name}\n\n"
            continue

        # Check for chapter links (with or without list marker)
        # Match: - [Title](path.md) or [Title](path.md)
        link_match = re.match(r'^(?:\s*-\s*)?\[([^\]]+)\]\(([^)]+\.md)\)', line)
        if link_match:
            title = link_match.group(1)
            path = link_match.group(2).lstrip('./')

            # Look up wiki name from page map
            if path in page_map:
                wiki_name = page_map[path]
                content += f"- [{title}]({wiki_name})\n"

    # Write Home.md
    home_path = WIKI_DIR / "Home.md"
    with open(home_path, 'w', encoding='utf-8') as f:
        f.write(content)

    print(f"Created: {home_path}")

def create_sidebar(chapters: list, page_map: dict):
    """Create the _Sidebar.md for wiki navigation."""

    content = """## Navigation

"""

    # Read SUMMARY.md to get section headers
    summary_path = BOOK_SRC / "SUMMARY.md"
    with open(summary_path, 'r', encoding='utf-8') as f:
        summary_content = f.read()

    lines = summary_content.split('\n')

    for line in lines:
        # Check for section headers
        section_match = re.match(r'^#\s+(.+)$', line)
        if section_match:
            section_name = section_match.group(1)
            if section_name != "Summary":
                content += f"\n**{section_name}**\n\n"
            continue

        # Check for chapter links (with or without list marker)
        link_match = re.match(r'^(?:\s*-\s*)?\[([^\]]+)\]\(([^)]+\.md)\)', line)
        if link_match:
            title = link_match.group(1)
            path = link_match.group(2).lstrip('./')

            # Look up wiki name from page map
            if path in page_map:
                wiki_name = page_map[path]
                content += f"- [{title}]({wiki_name})\n"

    # Write _Sidebar.md
    sidebar_path = WIKI_DIR / "_Sidebar.md"
    with open(sidebar_path, 'w', encoding='utf-8') as f:
        f.write(content)

    print(f"Created: {sidebar_path}")

def main():
    # Ensure wiki directory exists
    WIKI_DIR.mkdir(exist_ok=True)

    # Parse the book structure
    chapters = parse_summary()
    print(f"Found {len(chapters)} chapters")

    # Build page mapping
    page_map = build_page_map(chapters)

    # Clear existing wiki content (except .git)
    for item in WIKI_DIR.iterdir():
        if item.name == '.git':
            continue
        if item.is_file():
            item.unlink()
        elif item.is_dir():
            shutil.rmtree(item)

    # Create wiki pages
    create_wiki_pages(chapters, page_map)

    # Create Home.md
    create_home_page(chapters, page_map)

    # Create sidebar
    create_sidebar(chapters, page_map)

    print("Wiki sync complete!")

if __name__ == '__main__':
    main()
