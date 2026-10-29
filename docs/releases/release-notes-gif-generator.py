#!/usr/bin/env python3
"""
NoteCraft Release Notes GIF Generator

Generates animated GIF release notes by overlaying text on a background image.
Creates individual frames and assembles them into a GIF.

Usage:
    python release-notes-gif-generator.py <version>
    python release-notes-gif-generator.py 0.0.7

Requirements:
    - Pillow (PIL): pip install Pillow
    - imagemagick (convert command): Available on Windows via ImageMagick installer

Output:
    - <version>/.frames/  - Directory containing individual frame images
    - <version>/RELEASE_NOTES.gif - Final animated GIF
"""

import sys
import os
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import subprocess
import shutil

# Configuration
SCRIPT_DIR = Path(__file__).parent.absolute()
FONT_SIZE = 28
TITLE_FONT_SIZE = 48
LINE_HEIGHT = 40
FRAME_DURATION = 2000  # milliseconds per frame
TEXT_COLOR = (255, 255, 255)  # White text
BACKGROUND_COLOR = (0, 0, 0, 180)  # Semi-transparent black overlay


def get_available_font(fallback_size):
    """Try to load a nice font, fall back to default if unavailable."""
    font_paths = [
        "C:\\Windows\\Fonts\\segoeui.ttf",  # Windows
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",  # Linux
        "/System/Library/Fonts/Helvetica.ttc",  # macOS
    ]

    for font_path in font_paths:
        if Path(font_path).exists():
            try:
                return ImageFont.truetype(font_path, fallback_size)
            except Exception:
                pass

    # Fall back to default font
    return ImageFont.load_default()


def load_background(version):
    """Load the background image for the given version."""
    bg_path = SCRIPT_DIR / version / f"{version}.jfif"

    if not bg_path.exists():
        print(f"Error: Background image not found at {bg_path}")
        sys.exit(1)

    img = Image.open(bg_path)
    # Convert RGBA if needed
    if img.mode != "RGBA":
        img = img.convert("RGBA")

    print(f"✓ Loaded background: {bg_path} ({img.size})")
    return img


def create_frame_with_text(background, text_lines, frame_num):
    """Create a single frame with text overlay."""
    # Create a copy of the background
    frame = background.copy()

    # Create a semi-transparent overlay for text readability
    overlay = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    overlay_draw = ImageDraw.Draw(overlay)

    # Load fonts
    title_font = get_available_font(TITLE_FONT_SIZE)
    body_font = get_available_font(FONT_SIZE)

    # Calculate text positioning
    padding = 60
    start_y = 100

    # Draw semi-transparent background for text area
    text_height = start_y + (len(text_lines) * LINE_HEIGHT) + padding
    overlay_draw.rectangle(
        [(0, start_y - 30), (frame.size[0], text_height)],
        fill=(*BACKGROUND_COLOR[:3], int(BACKGROUND_COLOR[3] * 0.7))
    )

    # Draw text
    y_offset = start_y
    for i, line in enumerate(text_lines):
        # Use title font for first line, body font for rest
        font = title_font if i == 0 else body_font

        # Center text horizontally
        bbox = overlay_draw.textbbox((0, 0), line, font=font)
        text_width = bbox[2] - bbox[0]
        x_pos = (frame.size[0] - text_width) // 2

        overlay_draw.text(
            (x_pos, y_offset),
            line,
            font=font,
            fill=TEXT_COLOR
        )

        y_offset += LINE_HEIGHT

    # Composite overlay onto frame
    frame = Image.alpha_composite(frame, overlay)
    return frame


def generate_release_notes_frames(version, release_notes_content):
    """Generate individual frames from release notes content."""
    # Create frames directory
    frames_dir = SCRIPT_DIR / version / ".frames"
    frames_dir.mkdir(parents=True, exist_ok=True)

    # Clear existing frames
    for f in frames_dir.glob("*.png"):
        f.unlink()

    print(f"✓ Created frames directory: {frames_dir}")

    # Load background
    background = load_background(version)

    # Parse release notes into sections/slides
    # Format: Each slide is separated by ---
    slides = [
        slide.strip().split('\n')
        for slide in release_notes_content.split('---')
    ]

    # Filter empty slides
    slides = [s for s in slides if s and any(s)]

    if not slides:
        print("Error: No content to generate frames from")
        return frames_dir, []

    # Generate frames
    frame_paths = []
    for slide_num, text_lines in enumerate(slides, 1):
        # Limit lines per frame for readability
        text_lines = [line.strip() for line in text_lines if line.strip()][:6]

        frame = create_frame_with_text(background, text_lines, slide_num)

        frame_path = frames_dir / f"frame_{slide_num:03d}.png"
        frame.save(frame_path, "PNG")
        frame_paths.append(frame_path)

        print(f"  Frame {slide_num}: {frame_path.name}")

    print(f"✓ Generated {len(frame_paths)} frames")
    return frames_dir, frame_paths


def create_gif_from_frames(frames_dir, output_gif, frame_duration=FRAME_DURATION):
    """Create animated GIF from frames using ImageMagick or Pillow."""
    frame_paths = sorted(frames_dir.glob("frame_*.png"))

    if not frame_paths:
        print("Error: No frames found to create GIF")
        return False

    print(f"Creating GIF with {len(frame_paths)} frames...")

    # Try ImageMagick first (better quality)
    try:
        frames_pattern = str(frames_dir / "frame_*.png")
        cmd = [
            "magick",
            "convert",
            "-delay", str(frame_duration // 10),  # Convert ms to centiseconds
            "-loop", "0",  # Infinite loop
            frames_pattern,
            str(output_gif)
        ]

        # On Windows, might be called 'convert' without 'magick' prefix
        try:
            subprocess.run(cmd, check=True, capture_output=True)
        except FileNotFoundError:
            # Try alternate command name
            cmd[0] = "convert"
            subprocess.run(cmd, check=True, capture_output=True)

        print(f"✓ Created GIF using ImageMagick: {output_gif}")
        return True

    except (FileNotFoundError, subprocess.CalledProcessError) as e:
        print(f"ImageMagick not available: {e}")
        print("Falling back to Pillow GIF creation...")

        # Fallback: use Pillow
        try:
            images = [Image.open(p) for p in frame_paths]
            images[0].save(
                output_gif,
                save_all=True,
                append_images=images[1:],
                duration=frame_duration,
                loop=0,
                optimize=False
            )

            print(f"✓ Created GIF using Pillow: {output_gif}")
            return True

        except Exception as e:
            print(f"Error creating GIF with Pillow: {e}")
            return False


def main():
    if len(sys.argv) < 2:
        print("Usage: python release-notes-gif-generator.py <version>")
        print("Example: python release-notes-gif-generator.py 0.0.7")
        sys.exit(1)

    version = sys.argv[1]
    version_dir = SCRIPT_DIR / version

    if not version_dir.exists():
        print(f"Error: Version directory not found: {version_dir}")
        sys.exit(1)

    # Check for background image
    bg_path = version_dir / f"{version}.jfif"
    if not bg_path.exists():
        print(f"Error: Background image not found at {bg_path}")
        sys.exit(1)

    print(f"Generating release notes GIF for NoteCraft v{version}")
    print(f"Background: {bg_path}")

    # Define release notes content for this version
    # Format: Separate slides with ---
    release_notes_content = """NoteCraft v{version}
Release
---
RTL Text Handling
Improved caret positioning
in right-to-left content
---
GitHub Actions Fix
CI workflow now reports
status to branch protection
---
Installation & Testing
Complete fresh-profile
verification on Windows
---
Demo Parity Verified
HTML and CMGFM notebooks
identical in structure
---
Ready for Release
Full quality gate passed
""".format(version=version)

    # Generate frames
    frames_dir, frame_paths = generate_release_notes_frames(version, release_notes_content)

    if not frame_paths:
        print("Error: Failed to generate frames")
        sys.exit(1)

    # Create GIF
    output_gif = version_dir / "RELEASE_NOTES.gif"
    success = create_gif_from_frames(frames_dir, output_gif)

    if success:
        gif_size_mb = output_gif.stat().st_size / (1024 * 1024)
        print(f"\n✅ Success! Generated: {output_gif}")
        print(f"   Size: {gif_size_mb:.2f} MB")
        print(f"   Frames: {len(frame_paths)}")
        print(f"   Duration per frame: {FRAME_DURATION} ms")
        print(f"\nTo use in release notes:")
        print(f"![NoteCraft v{version} Release](https://raw.githubusercontent.com/AlienTechSoftware/notecraft/main/docs/releases/{version}/RELEASE_NOTES.gif)")
        return 0
    else:
        print("Error: Failed to create GIF")
        return 1


if __name__ == "__main__":
    sys.exit(main())
