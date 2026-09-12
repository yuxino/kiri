// Generated shared integration test. Maintain in yuxino/desktop-installer.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadBundle, themeDirectory } from './build.mjs';

test('pinned offline bundle matches this application and all three installer languages', () => {
  const files = loadBundle();
  const lock = JSON.parse(readFileSync(join(themeDirectory, 'theme.lock.json')));
  const config = JSON.parse(readFileSync(join(themeDirectory, '../tauri.windows.conf.json')));
  const nsis = config.bundle.windows.nsis;
  assert.equal(nsis.installerHooks, 'installer-theme/generated/theme.nsh');
  assert.equal(nsis.sidebarImage, 'installer-theme/generated/sidebar.bmp');
  assert.deepEqual(nsis.languages, ['English', 'SimpChinese', 'Japanese']);
  assert.equal(nsis.displayLanguageSelector, false);
  assert.equal(nsis.headerImage, undefined);
  assert.equal(nsis.uninstallerHeaderImage, undefined);
  for (const language of nsis.languages) {
    assert.equal(nsis.customLanguageFiles[language], `installer-theme/generated/${language}.nsh`);
    assert.equal((files[`${language}.nsh`].toString().match(/^LangString /gm) || []).length, 27);
  }
  const theme = files['theme.nsh'].toString();
  assert.equal((theme.match(/^LangString /gm) || []).length, 24);
  assert.ok(theme.includes(`MUI_FINISHPAGE_LINK_LOCATION "${lock.repository}"`));
  assert.doesNotMatch(theme, /^\s*(?:Section|Exec(?:Shell|Wait)?|WriteReg\w+|Delete|RMDir)\b/m);
  assert.doesNotMatch(theme, /!define MUI_PAGE_CUSTOMFUNCTION_(?:PRE|LEAVE)/);
  assert.match(theme, /user32::MapDialogRect/);
  assert.match(theme, /IfRebootFlag yuxino_finish_native/);
  assert.doesNotMatch(theme, /made with care|Small tools, softer days/);
});
