import { ref } from "vue";

export type Locale = "en" | "zh";

const STORAGE_KEY = "kfc-locale";

const messages: Record<string, Record<Locale, string>> = {
  "app.tagline": { en: "Safe, reusable local copy rules", zh: "安全、可复用的本地复制规则" },
  "nav.rules": { en: "Rules", zh: "规则" },
  "nav.jobs": { en: "History", zh: "历史" },
  "nav.aria": { en: "Views", zh: "视图" },
  "lang.aria": { en: "Language", zh: "语言" },

  "rules.title": { en: "Copy rules", zh: "复制规则" },
  "rules.empty": { en: "No rules yet. Create one to start copying.", zh: "还没有规则，新建一条即可开始复制。" },
  "rules.clickToEdit": { en: "Click to edit this rule", zh: "点击编辑此规则" },
  "rules.status.enabled": { en: "enabled", zh: "已启用" },
  "rules.status.disabled": { en: "disabled", zh: "已禁用" },

  "action.import": { en: "Import", zh: "导入" },
  "action.newRule": { en: "New rule", zh: "新建规则" },
  "action.export": { en: "Export", zh: "导出" },
  "action.duplicate": { en: "Duplicate", zh: "复制" },
  "action.disable": { en: "Disable", zh: "禁用" },
  "action.enable": { en: "Enable", zh: "启用" },
  "action.delete": { en: "Delete", zh: "删除" },
  "action.run": { en: "Run", zh: "运行" },
  "action.cancel": { en: "Cancel", zh: "取消" },
  "action.confirm": { en: "Confirm", zh: "确认" },
  "action.close": { en: "Close", zh: "关闭" },
  "action.save": { en: "Save", zh: "保存" },
  "action.refresh": { en: "Refresh", zh: "刷新" },
  "action.removeAll": { en: "Remove all", zh: "移除全部" },
  "action.browse": { en: "Browse", zh: "浏览" },
  "action.remove": { en: "Remove", zh: "移除" },

  "editor.title": { en: "Edit rule", zh: "编辑规则" },
  "editor.name": { en: "Rule name", zh: "规则名称" },
  "editor.source": { en: "Source", zh: "源" },
  "editor.chooseFile": { en: "Choose file", zh: "选择文件" },
  "editor.chooseFolder": { en: "Choose folder", zh: "选择文件夹" },
  "editor.destination": { en: "Destination", zh: "目标" },
  "editor.compare": { en: "Compare", zh: "比较" },
  "editor.compare.sizeAndMtime": { en: "Size and modified time", zh: "大小和修改时间" },
  "editor.compare.sizeOnly": { en: "Size only", zh: "仅大小" },
  "editor.overwrite": { en: "Overwrite", zh: "覆盖" },
  "editor.overwrite.ifChanged": { en: "If changed", zh: "有变更时" },
  "editor.overwrite.never": { en: "Never", zh: "从不" },
  "editor.overwrite.always": { en: "Always", zh: "始终" },
  "editor.trigger": { en: "Trigger", zh: "触发方式" },
  "editor.trigger.manual": { en: "Manual", zh: "手动" },
  "editor.trigger.watch": { en: "Watch changes", zh: "监听变更" },
  "editor.trigger.interval": { en: "Fixed interval", zh: "固定间隔" },
  "editor.trigger.cron": { en: "Cron", zh: "定时任务" },
  "editor.concurrency": { en: "Concurrency", zh: "并发数" },
  "editor.retries": { en: "Retries", zh: "重试次数" },
  "editor.debounceMs": { en: "Debounce (ms)", zh: "防抖（毫秒）" },
  "editor.intervalSeconds": { en: "Interval (seconds)", zh: "间隔（秒）" },
  "editor.cronExpression": { en: "Cron expression", zh: "定时表达式" },
  "editor.ruleEnabled": { en: "Rule enabled", zh: "启用规则" },
  "editor.includeHidden": { en: "Include hidden files", zh: "包含隐藏文件" },
  "editor.preserveMtime": { en: "Preserve modification times", zh: "保留修改时间" },
  "editor.destName": { en: "Destination name", zh: "目标文件名" },
  "editor.renamePatternPlaceholder": { en: "Source name pattern, e.g. ^(.+)_(\\d+\\.\\d+\\.\\d+\\.\\d+)_(.+)\\.(.+)$", zh: "源文件名模式，例如 ^(.+)_(\\d+\\.\\d+\\.\\d+\\.\\d+)_(.+)\\.(.+)$" },
  "editor.renamePatternAria": { en: "Source name pattern", zh: "源文件名模式" },
  "editor.renameDestPlaceholder": { en: "Destination template, e.g. $1_$2.$4", zh: "目标模板，例如 $1_$2.$4" },
  "editor.renameDestAria": { en: "Destination template", zh: "目标模板" },
  "editor.renameHint": { en: "Match the source file name with a regex; $1..$9 in the template become capture groups.", zh: "使用正则表达式匹配源文件名；模板中的 $1..$9 对应捕获组。" },
  "editor.autoJobHint": { en: "Automatic jobs run only while the desktop application is open.", zh: "自动任务仅在桌面应用打开时运行。" },
  "editor.addRename": { en: "Add rename rule", zh: "添加重命名规则" },

  "files.title": { en: "Files to copy", zh: "要复制的文件" },
  "files.modeAria": { en: "Selection mode", zh: "选择模式" },
  "files.mode.manual": { en: "Manual", zh: "手动" },
  "files.mode.group": { en: "Group", zh: "分组" },
  "files.browse": { en: "Browse source tree", zh: "浏览源目录树" },
  "files.manualHint": { en: "Check the files and folders to copy. Nothing is copied until you make a selection; unchecked entries are skipped.", zh: "勾选要复制的文件和文件夹。做出选择前不会复制任何内容；未勾选的条目将被跳过。" },
  "files.groupHint": { en: "Group mode extracts a version from each top-level file name; select the versions to copy.", zh: "分组模式从每个顶级文件名中提取版本号；选择要复制的版本。" },
  "files.filterPath": { en: "Filter source path", zh: "筛选源路径" },
  "files.filterEntriesAria": { en: "Filter source entries by path", zh: "按路径筛选源条目" },
  "files.layoutAria": { en: "Source layout", zh: "源布局" },
  "files.view.list": { en: "List", zh: "列表" },
  "files.view.tree": { en: "Tree", zh: "树" },
  "files.countFiles": { en: "{n} of {m} files", zh: "{m} 个文件中的 {n} 个" },
  "files.countEntries": { en: "{n} of {m} entries", zh: "{m} 个条目中的 {n} 个" },
  "files.treeAria": { en: "Files to copy", zh: "要复制的文件" },
  "files.col.name": { en: "Name", zh: "名称" },
  "files.col.size": { en: "Size", zh: "大小" },
  "files.col.modified": { en: "Modified", zh: "修改时间" },
  "files.col.group": { en: "Group", zh: "分组" },
  "files.noMatch": { en: "No files match the current filter.", zh: "没有文件与当前筛选匹配。" },
  "files.noMatchEntries": { en: "No source entries match the current filter.", zh: "没有源条目与当前筛选匹配。" },
  "files.labelOptional": { en: "Label (optional)", zh: "标签（可选）" },
  "files.groupLabelAria": { en: "Group label", zh: "分组标签" },
  "files.versionPattern": { en: "Version pattern, e.g. (\\d+\\.\\d+\\.\\d+\\.\\d+)", zh: "版本模式，例如 (\\d+\\.\\d+\\.\\d+\\.\\d+)" },
  "files.versionPatternAria": { en: "Version pattern", zh: "版本模式" },
  "files.addRule": { en: "Add rule", zh: "添加规则" },
  "files.filterGroup": { en: "Filter group or file", zh: "筛选分组或文件" },
  "files.filterGroupsAria": { en: "Filter groups and files", zh: "筛选分组和文件" },
  "files.groupLayoutAria": { en: "Group layout", zh: "分组布局" },
  "files.countGroups": { en: "{n} groups", zh: "{n} 个分组" },
  "files.groupsAria": { en: "File groups", zh: "文件分组" },
  "files.includeGroupAria": { en: "Include group {name}", zh: "包含分组 {name}" },
  "files.groupOther": { en: "Other files", zh: "其他文件" },
  "files.autoLatestGroup": { en: "Auto-select the newest group", zh: "自动选择最新分组" },
  "files.autoLatestHint": { en: "The group with the newest version is copied automatically; manual group selection is ignored.", zh: "将自动复制版本最新的分组；手动分组选择将被忽略。" },

  "preview.title": { en: "Dry-run preview", zh: "试运行预览" },
  "preview.button": { en: "Preview", zh: "预览" },
  "preview.buttonProgress": { en: "Previewing...", zh: "预览中…" },
  "preview.scanning": { en: "Scanning source · {n} entries found", zh: "正在扫描源 · 已找到 {n} 项" },
  "preview.building": { en: "Building plan · {n} / {m} entries", zh: "正在构建计划 · {n} / {m} 项" },
  "preview.stats": { en: "{files} files / {bytes} bytes to copy; {skipped} skipped; {warnings} warnings.", zh: "复制 {files} 个文件 / {bytes} 字节；跳过 {skipped} 个；{warnings} 个警告。" },
  "preview.filterPath": { en: "Filter path or reason", zh: "筛选路径或原因" },
  "preview.filterAria": { en: "Filter preview by path or reason", zh: "按路径或原因筛选预览" },
  "preview.filterActionAria": { en: "Filter preview by action", zh: "按操作筛选预览" },
  "preview.action.all": { en: "All actions", zh: "全部操作" },
  "preview.action.copyFile": { en: "Copy files", zh: "复制文件" },
  "preview.action.createDirectory": { en: "Create folders", zh: "创建文件夹" },
  "preview.action.skip": { en: "Skipped", zh: "已跳过" },
  "preview.action.warn": { en: "Warnings", zh: "警告" },
  "preview.countItems": { en: "{n} of {m} items", zh: "共 {m} 项中的 {n} 项" },
  "preview.col.action": { en: "Action", zh: "操作" },
  "preview.col.source": { en: "Source", zh: "源" },
  "preview.col.target": { en: "Target", zh: "目标" },
  "preview.col.description": { en: "Description", zh: "说明" },
  "preview.noMatch": { en: "No preview items match the current filter.", zh: "没有预览条目与当前筛选匹配。" },
  "preview.noPlanned": { en: "No planned operations.", zh: "没有计划的操作。" },

  "jobs.title": { en: "Job history", zh: "任务历史" },
  "jobs.current": { en: "Current job", zh: "当前任务" },
  "run.title": { en: "Copying…", zh: "复制中…" },
  "run.done": { en: "Copy finished", zh: "复制完成" },
  "run.failed": { en: "The copy job did not run.", zh: "复制任务未能运行。" },
  "jobs.progress": { en: "{completed} / {total} files · {path}", zh: "{completed} / {total} 个文件 · {path}" },
  "jobs.summary": { en: "{copied} copied · {skipped} skipped · {failed} failed · {bytes} bytes", zh: "已复制 {copied} 个 · 跳过 {skipped} 个 · 失败 {failed} 个 · {bytes} 字节" },
  "jobs.log": { en: "Detailed log", zh: "详细日志" },

  "dialog.discardTitle": { en: "Discard changes?", zh: "放弃更改？" },
  "dialog.discardMessage": { en: "You have unsaved changes to this rule. Discard them and leave the editor?", zh: "此规则有未保存的更改。放弃更改并离开编辑器吗？" },
  "dialog.deleteTitle": { en: "Delete rule?", zh: "删除规则？" },
  "dialog.deleteMessage": { en: "Delete rule “{name}”? Copied files will not be deleted.", zh: "删除规则“{name}”？已复制的文件不会被删除。" },
  "dialog.importConflictTitle": { en: "Rule already exists", zh: "规则已存在" },
  "dialog.importConflictMessage": { en: "A rule named “{name}” already uses this ID. Confirm to overwrite it, or Cancel to import a new copy.", zh: "已存在使用此 ID 的规则“{name}”。选择“确认”覆盖它，或选择“取消”导入为新副本。" },
  "dialog.removeAllTitle": { en: "Remove all history?", zh: "移除全部历史？" },
  "dialog.removeAllMessage": { en: "This removes every recorded job from history. This cannot be undone.", zh: "这将从历史中删除所有任务记录，且无法撤销。" },

  "msg.loadRecentPaths": { en: "Could not load recent paths: {error}", zh: "无法加载最近路径：{error}" },
  "msg.updateRecentPaths": { en: "Could not update recent paths: {error}", zh: "无法更新最近路径：{error}" },

  "rename.previewBrowse": { en: "Browse the source tree to preview the renamed name.", zh: "浏览源目录树以预览重命名后的名称。" },
  "rename.noMatch": { en: "{name} does not match the pattern; it will be copied as-is.", zh: "{name} 与模式不匹配，将按原样复制。" },

  "defaults.newRuleName": { en: "New copy rule", zh: "新复制规则" },
  "suffix.copy": { en: "copy", zh: "副本" },
  "suffix.imported": { en: "imported", zh: "导入" },
  "import.filterName": { en: "KFC rule", zh: "KFC 规则" },

  "tree.collapse": { en: "Collapse {name}", zh: "收起 {name}" },
  "tree.expand": { en: "Expand {name}", zh: "展开 {name}" },
  "tree.collapseFolder": { en: "Collapse folder", zh: "收起文件夹" },
  "tree.expandFolder": { en: "Expand folder", zh: "展开文件夹" },
  "tree.include": { en: "Include {path}", zh: "包含 {path}" },
  "tree.collapseGroup": { en: "Collapse group", zh: "收起分组" },
  "tree.expandGroup": { en: "Expand group", zh: "展开分组" },
  "group.copy": { en: "Copy {name}", zh: "复制 {name}" },
  "group.files": { en: "{n} files", zh: "{n} 个文件" },
};

function loadLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "zh" ? "zh" : "en";
  } catch {
    return "en";
  }
}

export const locale = ref<Locale>(loadLocale());

export function setLocale(next: Locale) {
  locale.value = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
    if (typeof document !== "undefined") document.documentElement.lang = next;
  } catch {
    // Storage or DOM unavailable (e.g. tests): locale still applies for this session.
  }
}

export function translate(localeValue: Locale, key: string, params?: Record<string, string | number>): string {
  const text = messages[key]?.[localeValue] ?? messages[key]?.en ?? key;
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) => String(params[name] ?? ""));
}

export function t(key: string, params?: Record<string, string | number>): string {
  return translate(locale.value, key, params);
}



