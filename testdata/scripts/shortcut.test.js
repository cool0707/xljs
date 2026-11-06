import { ok, equal, throws } from "./_assert.js";

// Tests run in an empty temporary directory.
fs.writeText("売上.xlsx", "x");
const file = fs.file("売上.xlsx");

// create: ".lnk" is added; paths may be handles.
const p = shell.writeShortcutLink("売上へのリンク", { target: file, args: "/r", description: "集計表", hotkey: "ctrl+alt+u", windowState: "maximized" });
ok(p.endsWith("売上へのリンク.lnk"), p);
ok(fs.exists(p));
let d = shell.readShortcutLink("売上へのリンク");
equal(d.target, file.path);
equal(d.args, "/r");
equal(d.description, "集計表");
equal(d.hotkey, "Ctrl+Alt+U");
equal(d.windowState, "maximized");
equal(d.iconIndex, 0);
equal(d.runAsAdmin, false);
equal(d.path, p);

// update changes only what is given.
shell.writeShortcutLink(p, "update", { args: "", runAsAdmin: true, icon: file, iconIndex: 3 });
d = shell.readShortcutLink(fs.file(p));
equal(d.args, "");
equal(d.description, "集計表");
equal(d.runAsAdmin, true);
equal(d.icon, file.path);
equal(d.iconIndex, 3);

// replace starts over (the shortcut must exist).
shell.writeShortcutLink(p, "replace", { target: "売上.xlsx" });
d = shell.readShortcutLink(p);
equal(d.target, file.path);
equal(d.description, "");
equal(d.hotkey, "");
throws(() => shell.writeShortcutLink("ない.lnk", "update", { args: "x" }), /ない\.lnk/);
throws(() => shell.writeShortcutLink("x", { description: "no target" }), /target/);
throws(() => shell.writeShortcutLink("x", "move", { target: file }), /move/);
throws(() => shell.writeShortcutLink("x", { target: file, hotkey: "Win+K" }), /Win\+K/);
throws(() => shell.readShortcutLink("売上.xlsx"), /ショートカット/);

// URLs become Internet shortcuts (.url).
const u = shell.writeShortcutLink("サイト", { target: "https://example.com/" });
ok(u.endsWith("サイト.url"), u);
equal(shell.readShortcutLink("サイト").target, "https://example.com/");

ok(typeof fs.knownFolder("desktop") === "string");
equal(fs.knownFolder("nowhere"), null);

console.log("shortcut ok");
