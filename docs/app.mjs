const repo = 'ufocoder/speech-to-text';
async function currentPlatform() {
  const info = await navigator.userAgentData?.getHighEntropyValues(['architecture', 'bitness']).catch(() => null);
  if (info?.platform === 'macOS') {
    if (info.architecture === 'arm') return 'macos-arm64';
    if (info.architecture === 'x86' && info.bitness === '64') return 'macos-x86_64';
  }
  if (info?.platform === 'Windows') {
    return info.architecture === 'x86' && info.bitness === '64' ? 'windows-x64' : null;
  }
  if (/Windows NT[^)]*(?:Win64; x64|WOW64)/.test(navigator.userAgent)) return 'windows-x64';
  return null;
}

const platformLabels = {
  'macos-arm64': 'Apple Silicon',
  'macos-x86_64': 'Mac Intel',
  'windows-x64': 'Windows x64',
};

async function showDownloads() {
  const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`);
  if (!response.ok) return;
  const release = await response.json();
  for (const label of document.querySelectorAll('[data-app-version]')) {
    label.textContent = release.tag_name;
  }
  const platform = await currentPlatform();
  for (const link of document.querySelectorAll('[data-platform]')) {
    const name = `speech-to-text-${release.tag_name}-${link.dataset.platform}.zip`;
    const url = `https://github.com/${repo}/releases/download/${release.tag_name}/${name}`;
    const asset = release.assets.find(asset => asset.name === name && asset.browser_download_url === url);
    if (asset) {
      link.href = url;
      const size = Number.isFinite(asset.size) && asset.size > 0
        ? new Intl.NumberFormat('ru', { maximumFractionDigits: 2 }).format(asset.size / 1_000_000)
        : null;
      link.firstElementChild.textContent = size ? `Скачать · ${size} МБ` : 'Скачать ZIP';
      link.lastElementChild.textContent = '↓';
      if (platform === link.dataset.platform && size) {
        const button = document.querySelector('#primary-download');
        button.href = url;
        button.firstElementChild.textContent = `Скачать для ${platformLabels[platform]} · ${size} МБ`;
      }
    }
  }
}
showDownloads().catch(() => {
  // The links to GitHub Releases remain available if the request fails.
});
