import { pipeline, env } from 'https://cdn.jsdelivr.net/npm/@huggingface/transformers@3.7.2';

env.allowLocalModels = false;
env.backends.onnx.wasm.numThreads = 1;
env.backends.onnx.wasm.proxy = false;

let recognizer;
self.onmessage = async ({ data }) => {
  try {
    recognizer ??= await pipeline('automatic-speech-recognition', 'onnx-community/whisper-tiny', {
      device: 'wasm',
      dtype: 'q8',
      progress_callback(event) {
        if (event.status === 'progress') {
          self.postMessage({ status: `Загрузка модели · ${Math.floor(event.progress)}%` });
        }
      },
    });
    self.postMessage({ status: 'Распознаём аудио…' });
    const result = await recognizer(data, { language: 'russian', task: 'transcribe', chunk_length_s: 25, stride_length_s: 4 });
    self.postMessage({ text: result.text });
  } catch (error) {
    self.postMessage({ error: String(error) });
  }
};
