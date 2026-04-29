// MV3 background. Two responsibilities:
//
// 1. Auto-lock the vault after 15 minutes of inactivity. The popup pings
//    `rn.activity` whenever it opens or the user clicks copy; this script
//    schedules an alarm 15 min ahead and clears `rn.vault` (session
//    storage) when it fires.
// 2. browser.storage.session is cleared automatically when the browser
//    closes, so an ephemeral vault is the default. The alarm is the
//    in-session top-up.

const ALARM = 'rn.lock';
const IDLE_MS = 15 * 60 * 1000;

browser.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'activity') {
    armLock();
  }
});

browser.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === ALARM) {
    void browser.storage.session.remove('rn.vault');
  }
});

function armLock() {
  browser.alarms.create(ALARM, { when: Date.now() + IDLE_MS });
}

// On install, clear any leftovers from an older version.
browser.runtime.onInstalled.addListener(() => {
  void browser.storage.session.remove('rn.vault');
});
