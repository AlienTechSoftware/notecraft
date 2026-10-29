#!/usr/bin/env python3
"""
NoteCraft Release Notes GIF Generator - Professional Edition

Generates high-quality animated GIF release notes with clean typography
on a background image. Creates individual frames and assembles them into a GIF.

Usage:
    python release-notes-gif-generator.py <version>
    python release-notes-gif-generator.py 0.0.7

Requirements:
    - Pillow (PIL): pip install Pillow
    - imagemagick (convert command): Optional, for higher quality output

Output:
    - <version>/.frames/  - Directory containing individual frame images
    - <version>/RELEASE_NOTES.gif - Final animated GIF
"""

import sys
import os
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import subprocess

# Configuration
SCRIPT_DIR = Path(__file__).parent.absolute()
FONT_SIZE_TITLE = 52
FONT_SIZE_SUBTITLE = 36
FONT_SIZE_BODY = 26
LINE_HEIGHT = 50
FRAME_DURATION = 3000  # milliseconds per frame
TEXT_COLOR_PRIMARY = (255, 255, 255)  # White
TEXT_COLOR_ACCENT = (120, 180, 255)  # Light blue
SHADOW_OFFSET = 2


def get_available_font(size, bold=False):
    """Try to load a system font, fall back to default if unavailable."""
    font_paths = [
        ("C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\segoeuib.ttf"),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        ("/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Helvetica.ttc"),
    ]

    for regular, bold_path in font_paths:
        path = bold_path if bold else regular
        try:
            if Path(path).exists():
                return ImageFont.truetype(path, size)
        except Exception:
            pass

    return ImageFont.load_default()


def load_background(version):
    """Load and prepare the background image."""
    bg_path = SCRIPT_DIR / version / f"{version}.jfif"

    if not bg_path.exists():
        print(f"Error: Background image not found at {bg_path}")
        sys.exit(1)

    img = Image.open(bg_path)
    if img.mode != "RGBA":
        img = img.convert("RGBA")

    print(f"✓ Loaded background: {bg_path.name} ({img.size[0]}×{img.size[1]})")
    return img


def create_frame_with_text(background, text_lines, frame_num):
    """Create a single professional-looking frame."""
    frame = background.copy()
    width, height = frame.size
    draw = ImageDraw.Draw(frame)

    # Fonts
    title_font = get_available_font(FONT_SIZE_TITLE, bold=True)
    subtitle_font = get_available_font(FONT_SIZE_SUBTITLE)
    body_font = get_available_font(FONT_SIZE_BODY)

    # Add a subtle dark overlay at the top for better text contrast
    overlay = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    overlay_draw = ImageDraw.Draw(overlay)

    # Gradient overlay from top
    for y in range(int(height * 0.4)):
        alpha = int(50 * (1 - (y / (height * 0.4))))
        overlay_draw.rectangle([(0, y), (width, y + 1)], fill=(0, 0, 0, alpha))

    frame = Image.alpha_composite(frame, overlay)
    draw = ImageDraw.Draw(frame)

    # Text positioning
    padding_top = int(height * 0.12)
    y_offset = padding_top

    # Draw each line
    for i, line in enumerate(text_lines):
        line = line.strip()
        if not line:
            y_offset += int(LINE_HEIGHT * 0.3)
            continue

        # Determine styling based on position
        if i == 0:
            font = title_font
            color = TEXT_COLOR_ACCENT
            y_offset += int(LINE_HEIGHT * 0.1)
        elif i == 1:
            font = subtitle_font
            color = TEXT_COLOR_PRIMARY
            y_offset += int(LINE_HEIGHT * 0.05)
        else:
            font = body_font
            color = TEXT_COLOR_PRIMARY

        # Center horizontally
        bbox = draw.textbbox((0, 0), line, font=font)
        text_width = bbox[2] - bbox[0]
        x_pos = (width - text_width) // 2

        # Draw shadow (darker version slightly offset)
        shadow_color = (0, 0, 0, 180)
        draw.text(
            (x_pos + SHADOW_OFFSET, y_offset + SHADOW_OFFSET),
            line,
            font=font,
            fill=shadow_color
        )

        # Draw main text
        draw.text((x_pos, y_offset), line, font=font, fill=color)

        y_offset += LINE_HEIGHT

    return frame


def generate_frames(version, content):
    """Generate frame images from release notes content."""
    frames_dir = SCRIPT_DIR / version / ".frames"
    frames_dir.mkdir(parents=True, exist_ok=True)

    # Clear old frames
    for f in frames_dir.glob("*.png"):
        f.unlink()

    print(f"✓ Frames directory: {frames_dir.name}/")

    background = load_background(version)

    # Split by --- separator
    slides = [s.strip().split('\n') for s in content.split('---')]
    slides = [s for s in slides if s and any(s)]

    if not slides:
        print("Error: No content to generate")
        return [], frames_dir

    frame_paths = []
    print(f"Generating {len(slides)} frames:")

    for num, lines in enumerate(slides, 1):
        lines = [ln.strip() for ln in lines if ln.strip()][:4]
        frame = create_frame_with_text(background, lines, num)
        path = frames_dir / f"frame_{num:03d}.png"
        frame.save(path, "PNG")
        frame_paths.append(path)
        print(f"  {num}. {lines[0][:40]}")

    return frame_paths, frames_dir


def create_gif(frames_dir, output_path, duration=FRAME_DURATION):
    """Create GIF from frames."""
    frames = sorted(frames_dir.glob("frame_*.png"))
    if not frames:
        print("Error: No frames found")
        return False

    print(f"\nCreating GIF ({duration}ms per frame)...")

    # Try ImageMagick
    try:
        pattern = str(frames_dir / "frame_*.png")
        for cmd_name in ["magick", "convert"]:
            try:
                subprocess.run(
                    [cmd_name, "convert", "-delay", str(duration // 10), "-loop", "0",
                     pattern, str(output_path)],
                    check=True,
                    capture_output=True,
                    timeout=60
                )
                print(f"✓ ImageMagick: {output_path.name}")
                return True
            except FileNotFoundError:
                continue
            except subprocess.CalledProcessError:
                continue
    except Exception:
        pass

    # Fallback to Pillow
    try:
        images = [Image.open(p) for p in frames]
        images[0].save(
            output_path,
            save_all=True,
            append_images=images[1:],
            duration=duration,
            loop=0,
            optimize=False
        )
        print(f"✓ Pillow: {output_path.name}")
        return True
    except Exception as e:
        print(f"Error: {e}")
        return False


def main():
    if len(sys.argv) < 2:
        print("Usage: python release-notes-gif-generator.py <version>")
        print("Example: python release-notes-gif-generator.py 0.0.7")
        sys.exit(1)

    version = sys.argv[1]
    version_dir = SCRIPT_DIR / version

    if not version_dir.exists():
        print(f"Error: {version_dir} not found")
        sys.exit(1)

    print(f"NoteCraft v{version} Release Notes GIF\n")

    # Content
    content = f"""NoteCraft v{version}
Quality Release
---
RTL Text Handling
Caret improvements
in right-to-left scripts
---
GitHub Actions Fixed
CI workflow properly
reports to branch protection
---
Installation Testing
Complete fresh-profile
verification on Windows
---
Demo Parity Verified
HTML and CMGFM
render identically
---
Ready for Release
Full quality gate passed
"""

    # Generate
    frames, frames_dir = generate_frames(version, content)
    if not frames:
        sys.exit(1)

    output_gif = version_dir / "RELEASE_NOTES.gif"
    if not create_gif(frames_dir, output_gif):
        sys.exit(1)

    size_mb = output_gif.stat().st_size / (1024 * 1024)
    print(f"\n✅ {output_gif.name} ({size_mb:.2f} MB, {len(frames)} frames)")
    print(f"\nMarkdown:\n![Release]" +
          f"(https://raw.githubusercontent.com/AlienTechSoftware/notecraft/main/docs/releases/{version}/RELEASE_NOTES.gif)")


if __name__ == "__main__":
    main()
