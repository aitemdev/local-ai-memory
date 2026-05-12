import { ensureStore } from "./db.js";

export function getSettings(keys, options = {}) {
  const { db } = ensureStore(options.base);
  try {
    const select = db.prepare("SELECT value FROM settings WHERE key = ?");
    const result = {};
    for (const key of keys) result[key] = select.get(key)?.value;
    return result;
  } finally {
    db.close();
  }
}

export function setSettings(values, options = {}) {
  const { db } = ensureStore(options.base);
  try {
    const set = db.prepare(`
      INSERT INTO settings (key, value, updated_at)
      VALUES (?, ?, CURRENT_TIMESTAMP)
      ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP
    `);
    for (const [key, value] of Object.entries(values)) {
      if (value !== undefined) set.run(key, String(value));
    }
  } finally {
    db.close();
  }
}

export function listSettings(prefix, options = {}) {
  const { db } = ensureStore(options.base);
  try {
    return db.prepare("SELECT key, value, updated_at FROM settings WHERE key LIKE ? ORDER BY key").all(`${prefix}%`).map((row) => ({ ...row }));
  } finally {
    db.close();
  }
}
