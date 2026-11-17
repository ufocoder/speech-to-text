# S2T (Speech-to-text)

S2T records audio and transcribes it on your computer. It is free and open source. Once you download the models, transcription works offline.

[Download S2T from GitHub Releases](https://github.com/ufocoder/speech-to-text/releases/latest)

| Platform | ZIP filename suffix |
| --- | --- |
| macOS (Apple Silicon) | `macos-arm64.zip` |
| macOS (Intel) | `macos-x86_64.zip` |
| Windows (64-bit) | `windows-x64.zip` |

![S2T desktop app interface](docs/image.webp)

## Recordings and transcripts

- Click the waveform to jump to any point in a recording.
- The recordings list shows audio files in the selected folder, their recording times, and the models used for saved transcripts.
- You can save transcripts from multiple models for the same audio file. Open the file and select **Past results →** to view them.
- In Settings, you can change the microphone, recordings folder, and interface language. The interface is available in English and Russian.

By default, S2T stores recordings in `recordings` and models in `.model-cache/hub`. The packaged macOS app puts these folders under `~/Library/Application Support/Speech to Text/`. On Windows, they are under `%LOCALAPPDATA%\Speech to Text\`.

## Development

[Build, test, preview the website, and publish releases](docs/development.md)

## License

S2T is distributed under the [MIT License](LICENSE).
