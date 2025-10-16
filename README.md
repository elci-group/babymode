# Babymode 👶

A Rust-based multimedia application that automatically detects and censors swearing in video files using faster-whisper for speech recognition. The application processes video files to automatically reduce audio volume during detected profanity while maintaining smooth audio transitions.

## Features

- 🎥 **Multi-format video support** - Works with MP4, AVI, MOV, MKV, WebM, and more
- 🎤 **Accurate speech recognition** - Uses faster-whisper for precise word-level timestamps
- 🔇 **Smart censoring** - Smooth volume reduction with configurable fade transitions
- 📝 **Customizable word list** - Define your own list of words to censor
- ⚡ **Fast processing** - Efficient audio processing with minimal quality loss
- 🛠️ **CLI interface** - Easy-to-use command-line interface

## Prerequisites

Before installing Babymode, ensure you have the following dependencies:

### System Dependencies

1. **FFmpeg** - Required for video/audio processing
   ```bash
   # Ubuntu/Debian
   sudo apt update && sudo apt install ffmpeg
   
   # macOS (with Homebrew)
   brew install ffmpeg
   
   # Windows (with Chocolatey)
   choco install ffmpeg
   ```

2. **Python 3.8+** with **faster-whisper**
   ```bash
   # Install Python dependencies
   pip install faster-whisper
   ```

3. **Rust** (for building from source)
   ```bash
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

## Installation

### From Source

1. Clone the repository:
   ```bash
   git clone <repository-url>
   cd babymode/project
   ```

2. Build the application:
   ```bash
   cargo build --release
   ```

3. The binary will be available at `target/release/babymode`

### Add to PATH (Optional)

```bash
# Add to your shell profile (.bashrc, .zshrc, etc.)
export PATH="$PATH:/path/to/babymode/project/target/release"
```

## Usage

### Basic Usage

```bash
# Process a video file with default settings
babymode -i input_video.mp4

# Specify output file
babymode -i input_video.mp4 -o clean_video.mp4

# Use verbose logging
babymode -i input_video.mp4 --verbose
```

### Advanced Options

```bash
# Custom whisper model (tiny, base, small, medium, large)
babymode -i input.mp4 -m small

# Adjust censoring volume (0.0 = silence, 1.0 = original volume)
babymode -i input.mp4 -v 0.05

# Adjust fade duration (in seconds)
babymode -i input.mp4 -f 0.3

# Custom word list
babymode -i input.mp4 -w "damn,hell,crap"
```

### Complete Example

```bash
# Process with all custom options
babymode \
  --input my_video.mp4 \
  --output censored_video.mp4 \
  --model base \
  --volume 0.1 \
  --fade 0.25 \
  --words "fuck,shit,damn,hell" \
  --verbose
```

## Configuration

### Default Word List

Babymode comes with a default list of common profanity:
- fuck, shit, damn, hell, ass, bitch, bastard

You can override this with the `--words` option.

### Whisper Models

Available models (trade-off between speed and accuracy):
- `tiny` - Fastest, least accurate
- `base` - Good balance (default)
- `small` - Better accuracy
- `medium` - High accuracy
- `large` - Best accuracy, slowest

### Audio Processing

- **Sample Rate**: 16kHz (optimal for Whisper)
- **Format**: WAV/PCM for processing, AAC for final output
- **Channels**: Converted to mono for analysis, original preserved in output

## How It Works

1. **Video Validation**: Checks input file format and accessibility
2. **Audio Extraction**: Extracts audio track from video using FFmpeg
3. **Speech Recognition**: Processes audio with faster-whisper for word-level timestamps
4. **Profanity Detection**: Identifies swear words and their precise timing
5. **Audio Censoring**: Applies smooth volume reduction during detected profanity
6. **Video Reconstruction**: Combines original video with censored audio track

## Supported Formats

### Input Formats
- **Video**: MP4, AVI, MOV, MKV, WMV, FLV, WebM, M4V, 3GP, MPG, MPEG
- **Audio**: Any format supported by FFmpeg

### Output Format
- **Video**: MP4 with H.264 video and AAC audio

## Performance Tips

1. **Choose appropriate model**: Use `tiny` or `base` for faster processing
2. **Hardware acceleration**: Ensure FFmpeg has hardware acceleration enabled
3. **File location**: Process files on local storage (not network drives)
4. **Available RAM**: Larger models require more memory

## Troubleshooting

### Common Issues

**Error: "ffmpeg not found"**
- Install FFmpeg and ensure it's in your system PATH

**Error: "Python process failed"**
- Install Python 3.8+ and faster-whisper: `pip install faster-whisper`

**Error: "Invalid video format"**
- Check that your video file isn't corrupted and uses a supported format

**Slow processing**
- Try using a smaller whisper model (`tiny` or `base`)
- Ensure you have adequate RAM available

### Debug Mode

Run with `--verbose` flag to see detailed processing information:

```bash
babymode -i input.mp4 --verbose
```

## Development

### Project Structure

```
src/
├── lib.rs           # Library interface and exports
├── main.rs          # CLI interface and main application logic
├── config.rs        # Configuration management with builder pattern
├── error.rs         # Custom error types and handling
├── resources.rs     # RAII resource management (TempFile, etc.)
├── video.rs         # Video processing and validation
├── audio.rs         # Audio extraction and processing
├── whisper.rs       # Speech recognition and word detection
└── censoring.rs     # Audio censoring logic
```

### Running Tests

```bash
cargo test
```

### Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [faster-whisper](https://github.com/guillaumekln/faster-whisper) for speech recognition
- [FFmpeg](https://ffmpeg.org/) for multimedia processing
- The Rust community for excellent crates and tools

## Changelog

### v0.1.0
- Initial release
- Basic video processing and profanity censoring
- CLI interface with configurable options
- Support for multiple video formats
- Smooth audio transitions during censoring

---

**Note**: This tool is designed for content moderation and parental controls. Please use responsibly and in accordance with applicable laws and regulations.