const file = document.querySelector('#file');
const player = document.querySelector('#player');
const run = document.querySelector('#run');
const status = document.querySelector('#status');
const result = document.querySelector('#result');
let objectUrl;
let worker;

file.onchange = () => {
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  const selected = file.files[0];
  objectUrl = selected ? URL.createObjectURL(selected) : undefined;
  player.hidden = !selected;
  if (objectUrl) player.src = objectUrl;
  else player.removeAttribute('src');
  result.value = '';
  status.textContent = '';
  run.disabled = !selected;
};

function finish(message) {
  status.textContent = message;
  file.disabled = false;
  run.disabled = false;
}

run.onclick = async () => {
  const selected = file.files[0];
  if (!selected) return;
  file.disabled = run.disabled = true;
  status.textContent = 'Подготавливаем аудио…';
  result.value = '';
  try {
    const decoder = new AudioContext();
    let audio;
    try {
      audio = await decoder.decodeAudioData(await selected.arrayBuffer());
    } finally {
      await decoder.close();
    }
    const offline = new OfflineAudioContext(1, Math.max(1, Math.ceil(audio.duration * 16000)), 16000);
    const source = offline.createBufferSource();
    source.buffer = audio;
    source.connect(offline.destination);
    source.start();
    const samples = (await offline.startRendering()).getChannelData(0);
    if (!worker) {
      worker = new Worker('./worker.mjs', { type: 'module' });
      worker.onmessage = ({ data }) => {
        if (data.status) status.textContent = data.status;
        else if (data.error) finish(`Ошибка распознавания: ${data.error}`);
        else {
          result.value = data.text;
          finish(data.text ? 'Готово' : 'Речь не распознана');
        }
      };
      worker.onerror = () => {
        worker.terminate();
        worker = undefined;
        finish('Не удалось запустить WebAssembly. Проверьте подключение и повторите попытку.');
      };
    }
    status.textContent = 'Загрузка движка и модели…';
    worker.postMessage(samples, [samples.buffer]);
  } catch (error) {
    finish(`Не удалось подготовить аудио: ${error.message}`);
  }
};

async function showDownloads() {
  const repo = 'ufocoder/speech-to-text';
  const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`);
  if (!response.ok) throw new Error('Release request failed');
  const release = await response.json();
  const platforms = {
    'windows-x64': 'Windows x64',
    'macos-arm64': 'macOS · Apple Silicon',
    'macos-x86_64': 'macOS · Intel',
  };
  for (const [platform, label] of Object.entries(platforms)) {
    const name = `speech-to-text-${release.tag_name}-${platform}.zip`;
    const url = `https://github.com/${repo}/releases/download/${release.tag_name}/${name}`;
    if (!release.assets.some(asset => asset.name === name && asset.browser_download_url === url)) continue;
    const link = document.createElement('a');
    link.className = 'download';
    link.href = url;
    link.textContent = label;
    document.querySelector('#downloads').prepend(link);
  }
}

showDownloads().catch(() => {
  document.querySelector('#release-status').textContent = 'Список сборок доступен на GitHub.';
});
