// 自分の所属するグループのアカウントを点検して Excel に書き出す（ドメインに参加している Windows で実行）。
// 無効・ロックアウト中・パスワード期限切れのユーザーと、90 日ログオンのないコンピューターをシートに分ける。
//
// ドメイン全体は検索しない。ad.search は条件に合うものをすべて読むので、条件の緩い検索を大きなドメインで行うと
// 時間がかかり、ドメインコントローラーにも負荷をかける。memberOf・searchBase・limit で範囲を絞ること。
if (!ad.available()) {
  alert("このパソコンはドメインに参加していないため、Active Directory を検索できません。");
  exit();
}

/** 1 つのグループから読むメンバーの上限（全社員のグループなどを読み切らないため） */
const LIMIT = 500;
/** この日数ログオンのないコンピューターを「使われていない」とする */
const INACTIVE_DAYS = 90;

const me = ad.me();
const groups = me.memberOf || [];
if (!groups.length) {
  alert("所属しているグループがありません。");
  exit();
}

/** 識別名の先頭の名前（"CN=営業部,OU=..." → "営業部"） */
const groupName = (/** @type {string} */ dn) =>
  (dn.match(/^[^=]+=((?:\\.|[^,])+)/)?.[1] ?? dn).replace(/\\(.)/g, "$1");

// グループごとに 1 回だけ検索し、状態の振り分けは読んだ結果で行う（条件ごとに検索し直さない）。
// 進捗を出し、キャンセルされたらそこで終了する。
/** @type {Map<string, { obj: ADObject, groups: string[] }>} */
const members = new Map();
/** @type {string[]} */
const truncated = [];
for (const dn of ui.withProgress(groups, { title: "Active Directory を検索中", label: groupName })) {
  const found = ad.search({ type: ["user", "computer"], memberOf: dn, limit: LIMIT });
  if (found.length >= LIMIT) truncated.push(groupName(dn));
  for (const obj of found) {
    const m = members.get(obj.distinguishedName);
    if (m) m.groups.push(groupName(dn));
    else members.set(obj.distinguishedName, { obj, groups: [groupName(dn)] });
  }
}

const all = [...members.values()];
const users = all.filter((m) => m.obj.type === "user");
const computers = all.filter((m) => m.obj.type === "computer");
const since = dayjs().subtract(INACTIVE_DAYS, "day").toDate();

/** @param {{ obj: ADObject, groups: string[] }[]} list */
const userRows = (list) =>
  list.map(({ obj: u, groups }) => ({
    名前: u.displayName || u.name,
    ログオン名: u.sAMAccountName,
    部署: u.department || "",
    最終ログオン: u.lastLogonTimestamp || null,
    パスワード変更: u.pwdLastSet || null,
    グループ: groups.join("、"),
    場所: u.distinguishedName,
  }));

const sheets = [
  { name: "無効", rows: userRows(users.filter((m) => m.obj.enabled === false)) },
  { name: "ロックアウト", rows: userRows(users.filter((m) => m.obj.lockedOut)) },
  { name: "パスワード期限切れ", rows: userRows(users.filter((m) => m.obj.passwordExpired && m.obj.enabled !== false)) },
  {
    name: "使われていないPC",
    rows: computers
      .filter((m) => m.obj.enabled !== false && !(m.obj.lastLogonTimestamp && m.obj.lastLogonTimestamp > since))
      .map(({ obj: c, groups }) => ({
        名前: c.name,
        OS: c.operatingSystem || "",
        最終ログオン: c.lastLogonTimestamp || null,
        グループ: groups.join("、"),
        場所: c.distinguishedName,
      })),
  },
];

const summary = [
  `${groups.length} グループ、${users.length} ユーザー、${computers.length} コンピューターを調べました。`,
  ...sheets.map((s) => `${s.name}: ${s.rows.length} 件`),
];
if (truncated.length) summary.push(`メンバーが多いため先頭 ${LIMIT} 件だけ調べたグループ: ${truncated.join("、")}`);

if (!sheets.some((s) => s.rows.length)) {
  alert(summary.join("\n"));
  exit();
}

const dest = await showSaveFilePicker({ suggestedName: `アカウント点検_${dayjs().format("YYYYMMDD")}.xlsx` });
if (!dest) exit();
const book = workbooks.create();
for (const s of sheets) {
  const sheet = book.addSheet(s.name);
  if (s.rows.length) sheet.write(s.rows);
  else sheet.cell("A1").value = "該当なし";
}
book.save(dest);
alert(summary.join("\n"));
shell.showItemInFolder(dest);
