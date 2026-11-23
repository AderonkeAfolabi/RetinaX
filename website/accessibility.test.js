const { test, describe } = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

describe('WCAG 2.1 Accessibility Compliance (#289)', () => {
  const htmlPath = path.join(__dirname, 'index.html');
  const cssPath = path.join(__dirname, 'styles.css');
  const html = fs.readFileSync(htmlPath, 'utf8');
  const css = fs.readFileSync(cssPath, 'utf8');

  test('index.html contains skip-to-content link pointing to main landmark', () => {
    assert.match(html, /<a\s+href="#main-content"\s+class="skip-link">Skip to main content<\/a>/i);
    assert.match(html, /<main\s+id="main-content">/i);
    assert.match(html, /<\/main>/i);
  });

  test('interactive simulator tabs implement proper WAI-ARIA tab and tabpanel roles', () => {
    assert.match(html, /role="tablist"/i);
    assert.match(html, /id="tab-rbac"[^>]*role="tab"[^>]*aria-controls="panel-rbac"/i);
    assert.match(html, /id="panel-rbac"[^>]*role="tabpanel"[^>]*aria-labelledby="tab-rbac"/i);

    assert.match(html, /id="tab-zk"[^>]*role="tab"[^>]*aria-controls="panel-zk"/i);
    assert.match(html, /id="panel-zk"[^>]*role="tabpanel"[^>]*aria-labelledby="tab-zk"/i);

    assert.match(html, /id="tab-ai"[^>]*role="tab"[^>]*aria-controls="panel-ai"/i);
    assert.match(html, /id="panel-ai"[^>]*role="tabpanel"[^>]*aria-labelledby="tab-ai"/i);

    assert.match(html, /id="tab-fhir"[^>]*role="tab"[^>]*aria-controls="panel-fhir"/i);
    assert.match(html, /id="panel-fhir"[^>]*role="tabpanel"[^>]*aria-labelledby="tab-fhir"/i);

    assert.match(html, /id="tab-fetch"[^>]*role="tab"[^>]*aria-controls="panel-fetch"/i);
    assert.match(html, /id="panel-fetch"[^>]*role="tabpanel"[^>]*aria-labelledby="tab-fetch"/i);
  });

  test('all form inputs have associated labels and ARIA descriptors', () => {
    assert.match(html, /<label[^>]*for="input-patient-addr"/i);
    assert.match(html, /<label[^>]*for="rbac-action"/i);
    assert.match(html, /<label[^>]*for="rbac-permission"/i);
    assert.match(html, /<label[^>]*for="rbac-role"/i);
    assert.match(html, /<label[^>]*for="wallet-network-select"/i);
    assert.match(html, /<label[^>]*for="dur-slider"/i);
    assert.match(html, /<label[^>]*for="acuity-slider"/i);
    assert.match(html, /<label[^>]*for="select-ai-status"/i);
    assert.match(html, /<label[^>]*for="select-fhir-type"/i);
  });

  test('heading hierarchy avoids arbitrary skipping in footer', () => {
    assert.match(html, /<nav class="footer-col"[^>]*>\s*<h3>Protocol<\/h3>/i);
    assert.match(html, /<nav class="footer-col"[^>]*>\s*<h3>Ecosystem<\/h3>/i);
    assert.match(html, /<nav class="footer-col"[^>]*>\s*<h3>Open Source<\/h3>/i);
  });

  test('styles.css contains visible focus rings and prefers-reduced-motion', () => {
    assert.match(css, /:focus-visible/i);
    assert.match(css, /@media\s*\(\s*prefers-reduced-motion\s*:\s*reduce\s*\)/i);
  });
});

describe('WCAG AA Color Contrast Compliance (#474)', () => {
  const baseCssPath = path.join(__dirname, 'components', 'base.css');
  const baseCss = fs.readFileSync(baseCssPath, 'utf8');

  function hexToRgb(hex) {
    const clean = hex.replace('#', '').trim();
    const bigint = parseInt(clean, 16);
    return [(bigint >> 16) & 255, (bigint >> 8) & 255, bigint & 255];
  }

  function relativeLuminance([r, g, b]) {
    const [rs, gs, bs] = [r, g, b].map((c) => {
      const s = c / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
  }

  function contrastRatio(hex1, hex2) {
    const l1 = relativeLuminance(hexToRgb(hex1));
    const l2 = relativeLuminance(hexToRgb(hex2));
    const lighter = Math.max(l1, l2);
    const darker = Math.min(l1, l2);
    return (lighter + 0.05) / (darker + 0.05);
  }

  function extractTokens(cssContent, selectorPattern) {
    const match = cssContent.match(selectorPattern);
    if (!match) return {};
    const block = match[1];
    const tokens = {};
    const regex = /(--[\w-]+)\s*:\s*(#[0-9a-fA-F]{6});/g;
    let m;
    while ((m = regex.exec(block)) !== null) {
      tokens[m[1]] = m[2];
    }
    return tokens;
  }

  const lightTokens = extractTokens(baseCss, /:root\s*\{([^}]+)\}/);
  const darkTokens = extractTokens(baseCss, /:root\[data-theme="dark"\]\s*\{([^}]+)\}/);

  test('light theme core text tokens meet WCAG AA contrast (>= 4.5:1) against background and surface', () => {
    assert.ok(lightTokens['--bg-page'], 'missing --bg-page');
    assert.ok(lightTokens['--bg-surface'], 'missing --bg-surface');

    const textTokens = ['--text-main', '--text-muted', '--text-light'];
    for (const token of textTokens) {
      assert.ok(lightTokens[token], `missing ${token}`);
      const ratioPage = contrastRatio(lightTokens[token], lightTokens['--bg-page']);
      const ratioSurface = contrastRatio(lightTokens[token], lightTokens['--bg-surface']);
      assert.ok(
        ratioPage >= 4.5,
        `Light theme ${token} against --bg-page must be >= 4.5:1, got ${ratioPage.toFixed(2)}:1`
      );
      assert.ok(
        ratioSurface >= 4.5,
        `Light theme ${token} against --bg-surface must be >= 4.5:1, got ${ratioSurface.toFixed(2)}:1`
      );
    }
  });

  test('light theme status and badge tokens meet WCAG AA contrast (>= 4.5:1) against their badge backgrounds', () => {
    const pairs = [
      ['--clinical-teal-dark', '--clinical-teal-light'],
      ['--status-green', '--status-green-bg'],
      ['--status-amber', '--status-amber-bg'],
      ['--status-rose', '--status-rose-bg'],
      ['--status-gray', '--status-gray-bg'],
    ];

    for (const [fg, bg] of pairs) {
      assert.ok(lightTokens[fg], `missing ${fg}`);
      assert.ok(lightTokens[bg], `missing ${bg}`);
      const ratio = contrastRatio(lightTokens[fg], lightTokens[bg]);
      assert.ok(
        ratio >= 4.5,
        `Light theme ${fg} against ${bg} must be >= 4.5:1, got ${ratio.toFixed(2)}:1`
      );
    }
  });

  test('dark theme core text tokens meet WCAG AA contrast (>= 4.5:1) against background and surface', () => {
    assert.ok(darkTokens['--bg-page'], 'missing dark --bg-page');
    assert.ok(darkTokens['--bg-surface'], 'missing dark --bg-surface');

    const textTokens = ['--text-main', '--text-muted', '--text-light'];
    for (const token of textTokens) {
      assert.ok(darkTokens[token], `missing dark ${token}`);
      const ratioPage = contrastRatio(darkTokens[token], darkTokens['--bg-page']);
      const ratioSurface = contrastRatio(darkTokens[token], darkTokens['--bg-surface']);
      assert.ok(
        ratioPage >= 4.5,
        `Dark theme ${token} against --bg-page must be >= 4.5:1, got ${ratioPage.toFixed(2)}:1`
      );
      assert.ok(
        ratioSurface >= 4.5,
        `Dark theme ${token} against --bg-surface must be >= 4.5:1, got ${ratioSurface.toFixed(2)}:1`
      );
    }
  });

  test('dark theme status and badge tokens meet WCAG AA contrast (>= 4.5:1) against their badge backgrounds', () => {
    const pairs = [
      ['--clinical-teal-dark', '--clinical-teal-light'],
      ['--status-green', '--status-green-bg'],
      ['--status-amber', '--status-amber-bg'],
      ['--status-rose', '--status-rose-bg'],
      ['--status-gray', '--status-gray-bg'],
    ];

    for (const [fg, bg] of pairs) {
      assert.ok(darkTokens[fg], `missing dark ${fg}`);
      assert.ok(darkTokens[bg], `missing dark ${bg}`);
      const ratio = contrastRatio(darkTokens[fg], darkTokens[bg]);
      assert.ok(
        ratio >= 4.5,
        `Dark theme ${fg} against ${bg} must be >= 4.5:1, got ${ratio.toFixed(2)}:1`
      );
    }
  });
});
