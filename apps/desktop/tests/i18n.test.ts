import { test } from "node:test";
import assert from "node:assert/strict";
import { setLocale, t, translate } from "../src/i18n";

test("translate returns English text", () => {
  assert.equal(translate("en", "action.run"), "Run");
  assert.equal(translate("en", "editor.trigger.manual"), "Manual");
});

test("translate returns Chinese text", () => {
  assert.equal(translate("zh", "action.run"), "运行");
  assert.equal(translate("zh", "editor.trigger.manual"), "手动");
});

test("translate interpolates named parameters", () => {
  assert.equal(
    translate("en", "dialog.deleteMessage", { name: "Backup" }),
    "Delete rule “Backup”? Copied files will not be deleted.",
  );
  assert.equal(
    translate("zh", "dialog.deleteMessage", { name: "备份" }),
    "删除规则“备份”？已复制的文件不会被删除。",
  );
  assert.equal(translate("en", "files.countGroups", { n: 3 }), "3 groups");
  assert.equal(translate("zh", "files.countGroups", { n: 3 }), "3 个分组");
});

test("translate falls back to the key for unknown keys", () => {
  assert.equal(translate("en", "missing.key"), "missing.key");
  assert.equal(translate("zh", "missing.key"), "missing.key");
});

test("t follows the active locale set by setLocale", () => {
  setLocale("zh");
  assert.equal(t("nav.rules"), "规则");
  setLocale("en");
  assert.equal(t("nav.rules"), "Rules");
});
