import { ok, equal, throws } from "./_assert.js";

// Programs: cmd.exe on Windows, sh elsewhere (tests run in an empty
// temporary directory).
const win = xljs.platform === "windows";
// On Windows, what cmd.exe and more write is in the console code page,
// which may not have Japanese: ASCII there.
const jp = (s, ascii) => (win ? ascii : s);
const echo2 = win ? "echo out& echo err 1>&2& exit /b 3" : "echo out; echo err >&2; exit 3";

// runSync: a command line goes to the shell; the exit code and the output.
let r = shell.runSync(echo2);
equal(r.exitCode, 3);
equal(r.ok, false);
equal(r.stdout.trim(), "out");
equal(r.stderr.trim(), "err");
equal(r.timedOut, false);
ok(r.pid > 0);

// run: the same, as a Promise.
r = await shell.run(win ? "exit /b 0" : "true");
equal(r.exitCode, 0);
equal(r.ok, true);

// Arguments are passed as they are (spaces, quotes); the program is
// started directly.
// (cmd.exe's echo writes in the console code page, which may not have
// Japanese: only ASCII there.)
if (win) {
  r = shell.runSync("cmd.exe", ["/d", "/c", "echo", "a b", 'say "hi"']);
  equal(r.stdout.trim(), '"a b" "say \\"hi\\""');
} else {
  r = shell.runSync("printf", ["%s|", "a b", 'say "hi"', "日本語"]);
  equal(r.stdout, 'a b|say "hi"|日本語|');
}

// input, cwd and env.
const dir = jp("作業", "work");
fs.mkdir(dir);
const cat = win ? "more" : "cat";
const text = jp("入力", "input");
r = shell.runSync(cat, { input: text + "\n" });
equal(r.stdout.trim(), text);
r = shell.runSync(win ? "cd" : "pwd", { cwd: dir });
equal(path.basename(r.stdout.trim()), dir);
r = shell.runSync(win ? "echo %XLJS_TEST%" : 'echo "$XLJS_TEST"', { env: { XLJS_TEST: jp("値", "value") } });
equal(r.stdout.trim(), jp("値", "value"));

// encoding "buffer" gives the bytes.
r = shell.runSync(win ? "echo abc" : "printf abc", { encoding: "buffer" });
ok(r.stdout instanceof Uint8Array);
equal(String.fromCharCode(...r.stdout).trim(), "abc");

// Output line by line, as it arrives (also in runSync).
const lines = [];
const three = win ? "echo 1& echo 2& echo 3" : "echo 1; echo 2; echo 3";
r = await shell.run(three, { onStdout: (l) => lines.push(l) });
equal(lines.join(","), "1,2,3");
const sync = [];
shell.runSync(three, { onStdout: (l) => sync.push(l.trim()) });
equal(sync.join(","), "1,2,3");

// stdout "ignore": nothing is captured.
r = shell.runSync(three, { stdout: "ignore" });
equal(r.stdout, "");

// timeout ends the program.
const t0 = Date.now();
r = await shell.run(win ? "ping -n 30 127.0.0.1 >nul" : "sleep 30", { timeout: 200 });
equal(r.timedOut, true);
equal(r.ok, false);
ok(Date.now() - t0 < 10000, "timeout");

// spawn: a running program; stdin, kill.
const p = shell.spawn(cat, [], { stdin: "pipe", shell: true });
ok(p instanceof ChildProcess);
ok(p.running);
p.write(text + "\n");
p.end();
r = await p.wait();
equal(r.stdout.trim(), text);
equal(p.running, false);
ok(p.wait() === p.wait());

const long = shell.spawn(win ? "ping -n 30 127.0.0.1 >nul" : "sleep 30");
long.kill();
r = await long.wait();
equal(r.ok, false);

// Errors: no program, bad options.
throws(() => shell.runSync("xljs-no-such-program", []), /xljs-no-such-program/);
await shell.run("xljs-no-such-program", []).then(() => ok(false, "rejects"), (e) => ok(/xljs-no-such-program/.test(e.message), e.message));
throws(() => shell.runSync(""), /コマンド/);
throws(() => shell.runSync("x", { window: "big" }), /window/);
throws(() => shell.runSync("x", { stdout: "file" }), /stdout/);
throws(() => shell.runSync("x", { cwd: "ないフォルダ" }), /ないフォルダ/);
console.log("process ok");
