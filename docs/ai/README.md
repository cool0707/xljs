# AI チャットでスクリプトを作る

Claude・Gemini・Copilot などの AI チャット（無料版や軽量モデルを含む）に xljs のスクリプトを書いてもらうための資料です。
AI は xljs を知らないので、そのままだと Node.js 用のコード（`require("fs")` など）を書いてしまいます。最初にこの資料を渡してください。

| ファイル | 内容 | 大きさ |
|---|---|---|
| [`prompt.md`](prompt.md) | AI への指示（回答の決まり・作るものの決め方・よくある間違い）＋要点のリファレンス＋回答例（ツールとアプリ）。**これだけで基本的な集計・変換のツールと、一覧・登録のあるアプリは書ける** | 約 15,000 字 |
| [`reference.md`](reference.md) | 詳しいリファレンス（全オプション・書式・フォーム・表・画面の設計の方針・アプリモード）とレシピ 6 本 | 約 61,000 字 |
| [`copilot/instructions.md`](copilot/instructions.md) | Copilot のエージェント用の指示（`prompt.md` を 8,000 字以内に縮めたもの） | 約 7,700 字 |
| [`chatgpt/instructions.md`](chatgpt/instructions.md)・[`gemini/instructions.md`](gemini/instructions.md)・[`claude/instructions.md`](claude/instructions.md) | ChatGPT・Gemini・Claude 用の指示（3 つとも同じ中身。`copilot/instructions.md` のスキルの呼び方を、知識のファイル `xljs-reference.txt` などに替えたもの） | 約 7,800 字 |
| [`copilot/skills/xljs-reference/SKILL.md`](copilot/skills/xljs-reference/SKILL.md) | スキル版のリファレンス（`reference.md` から下の 6 つを除いたもの） | 約 26,000 字 |
| [`copilot/skills/xljs-excel/SKILL.md`](copilot/skills/xljs-excel/SKILL.md) | スキル版のリファレンス: Excel（ブック・シート・セルの読み書き、表の読み書き、セルの検索、結合セル、書式） | 約 5,700 字 |
| [`copilot/skills/xljs-app-design/SKILL.md`](copilot/skills/xljs-app-design/SKILL.md) | スキル版のリファレンス: 画面の設計（ツールかアプリかの決め方、フォームアプリ・カスタムアプリの方針、型、アンチパターン）とアプリモード | 約 18,000 字 |
| [`copilot/skills/xljs-pdf/SKILL.md`](copilot/skills/xljs-pdf/SKILL.md) | スキル版のリファレンス: PDF 出力と PDF の編集 | 約 5,000 字 |
| [`copilot/skills/xljs-sqlite/SKILL.md`](copilot/skills/xljs-sqlite/SKILL.md) | スキル版のリファレンス: SQLite | 約 3,300 字 |
| [`copilot/skills/xljs-network/SKILL.md`](copilot/skills/xljs-network/SKILL.md) | スキル版のリファレンス: Web API（fetch）と Active Directory | 約 3,500 字 |
| [`copilot/skills/xljs-chart/SKILL.md`](copilot/skills/xljs-chart/SKILL.md) | スキル版のリファレンス: グラフ（ui.chart） | 約 2,900 字 |
| [`copilot/skills/vba-port/SKILL.md`](copilot/skills/vba-port/SKILL.md) | Excel マクロ（VBA）移植用のスキル。素直な書き方への置き換え、不要なコードの省略、手順の見直しの提案 | 約 5,400 字 |
| [`coding-agent/xljs-dev/SKILL.md`](coding-agent/xljs-dev/SKILL.md) | 開発モードを使わず、自分の AI 開発環境（Claude Code・GitHub Copilot のエージェント・Cursor など）で作るときのスキル。開発モードがしていること（履歴、モックの UI での実行とエラーの読み取り、テストの再生、exe の作成）を AI に自分で行わせる | 約 5,900 字 |

配布 zip にも `prompt/` として同梱しています（このフォルダの中身がそのまま入ります）。

## 使い方

### どのチャットでも（貼り付け）

1. `prompt.md` を開いて全文をコピーし、新しいチャットの最初のメッセージとして貼り付けて送信する。
2. 続けて、やりたいことを書く（下の「頼み方のコツ」）。
3. 返ってきたコードを `.js` ファイル（例: `集計.js`、文字コードは UTF-8）に保存し、`xljs.exe` にドラッグ＆ドロップして実行する。

入力欄の文字数に上限があるチャット（Microsoft 365 Copilot など）では `prompt.md` だけを貼ってください。ファイルを添付できるなら `reference.md` を添付すると、書式・フォームなど細かい指定も正確になります。

### 毎回貼らなくてよい設定

| サービス | 設定する場所 |
|---|---|
| Claude | プロジェクトを作り、「指示」に `claude/instructions.md`、「ファイル（ナレッジ）」に下の 8 つのファイル |
| Gemini | Gem（スキル）を作り、「カスタム指示」に `gemini/instructions.md`、「知識」に下の 8 つのファイル |
| ChatGPT | GPT（またはプロジェクト）の「指示」に `chatgpt/instructions.md`（指示は 8,000 字まで）、「知識」に下の 8 つのファイル |
| Microsoft 365 Copilot | エージェントを作れる場合は「指示」に `copilot/instructions.md`（8,000 字の上限に収まる）、スキルに `copilot/skills/` の 8 つ（スキルは 30,000 字までなので、リファレンスは機能ごとに分けてある。スキルが使えなければ「知識」に `reference.md`） |
| GitHub Copilot（VS Code） | スクリプトのフォルダに `.github/copilot-instructions.md` として `prompt.md` を置く。`xljs init` で型定義も出すと補完が効く |

ChatGPT・Gemini・Claude の「知識」には、`copilot/skills/` の 8 つの `SKILL.md` を `<フォルダ名>.txt`（`xljs-reference.txt`・`xljs-excel.txt`・`xljs-app-design.txt`・`xljs-pdf.txt`・`xljs-sqlite.txt`・`xljs-network.txt`・`xljs-chart.txt`・`vba-port.txt`）に名前を変えて入れます。指示がこの名前で呼びます。拡張子を .txt にするのは、.md だと「サポートされていないファイル形式」になることがあるためです（Gemini のスキルなど。Windows に .md の種類の登録がないと、ブラウザーがファイルの種類を空にする）。

開発モード（`xljs dev`）では、これらのサービスのふつうのチャットを開くと「xljs 用のエージェント（Gem・GPT・プロジェクト）を作成しましょう」と案内が出ます。［作成用のプロンプト］で、名前・説明の例、指示のコピー、スキル・知識のファイル（チャットの画面へドラッグして添付）をまとめて表示します。

## 自分の AI 開発環境で作る（開発モードを使わないとき）

ファイルを読み書きしてコマンドを実行できる AI（Claude Code、VS Code の GitHub Copilot のエージェント、Cursor、Codex など）で作るときは、開発モードの代わりに `coding-agent/xljs-dev/SKILL.md` を渡します。AI が、書き換える前の履歴の保存、`xljsc check` と画面を出さない実行（`--ui=mock`）での確認、エラーの読み取り、テスト（`.xljs/tests`）の再生を自分で行うようになります（テストの承認は利用者が行います）。

1. スクリプトのフォルダで `xljsc init` を実行し、型定義（`xljs.d.ts`）を置く。AI はこれで API を確かめます。
2. スキルを置く。xljs の API はほかのスキル（`copilot/skills/` の 8 つ）が持つので、一緒に置きます。
   - Claude Code: `.claude/skills/xljs-dev/SKILL.md`、ほかは `.claude/skills/<フォルダ名>/SKILL.md`
   - GitHub Copilot（VS Code）: `.github/skills/` の下に同じ形で置く
   - スキルを使えないツール: `AGENTS.md`（または `CLAUDE.md`・`.github/copilot-instructions.md`）に `prompt.md` と `xljs-dev/SKILL.md` の本文（先頭の `---` の間を除く）を貼り、`reference.md` を同じフォルダに置く
3. `xljsc.exe` に PATH が通っていなければ、場所を AI に伝える。

## Excel マクロ（VBA）を移植する

1. マクロ付きのブック（.xlsm など）を `xljs.exe` にドラッグ＆ドロップし、VBA コードをテキストファイルに保存する（コマンドなら `xljs vba ブック.xlsm`）。数式・入力規則・名前の定義・条件付き書式・グラフにもチェックを付けると、シートの仕組みも一緒に書き出せます（コマンドなら `xljs vba --all ブック.xlsm`）。
2. `prompt.md` を貼ったチャットに、「この Excel マクロを xljs に移植してください」と書いて、保存したテキストを貼る（または添付する）。
   スキルを使えるチャット（Copilot のエージェント、Claude のスキルなど）には `copilot/skills/vba-port/SKILL.md` を登録し、使えないチャットでは `prompt.md` に続けて貼るか添付してください。
3. 入力ブックの形（シート名・見出し・列名）も書くと確実です。マクロの中身は AI がまず説明するので、意図どおりか確かめてから使ってください。

## 頼み方のコツ

- **入力ファイルの形を具体的に書く**: シート名、見出しが何行目か、列名（「日付・顧客名・商品・数量・金額」）、1 ファイルか複数か。見出しと数行を（個人情報を消して）貼るのがいちばん確実です。
- **出力の形を書く**: 「顧客ごとの件数と合計を新しい Excel に」「Shift_JIS の CSV で」「元のファイルと同じフォルダに」など。
- **誰がどう使うかを書く**: 「自分だけ」か「共有フォルダに置いて何人かで使う」か、実行する人の氏名・所属を使うか（AI は Active Directory から取るコードを書きます）、データを次回に持ち越すか。開発モードの新規作成ウィザードでは「使う人」「データの保存」で選べます。
- **作りたいものを書く**: xljs では、いくつかの項目に入力して実行する「ツール」（集計・変換・帳票の作成）も、開いたまま使う「アプリ」（一覧・検索・登録・更新、複数の機能）も作れます。どちらも HTML を書かないフォーム（`ui.form`）の画面です。AI は依頼からどちらかを決めるので、「一覧を見ながら登録・修正・削除したい」「登録と照会の 2 つの画面がほしい」のように、続けて使うものならそう書きます。
- **画面の流れを書く**: 「開いたら自分の担当分の一覧が出て、チェックを付けて登録する」のように、最初に見えるものと操作を書くと、1 つの画面にまとまります。
- **画面が要らないときはそう書く**: タスクスケジューラーや他のアプリから自動で実行するなら「画面を出さずに自動実行したい」、一度きりの作業なら「使い捨てでよい」と書くと、フォームなしで書きます。
- **一度に頼みすぎない**: まず動くものを作ってもらい、「金額の列に 3 桁区切りを付けて」のように足していきます。
- **エラーが出たら**: エラーダイアログの「コピー」ボタンで全文をコピーし、「このエラーが出ました」と貼って直してもらいます。
- 実行する前に構文だけ確かめるには `xljs check 集計.js`。
- 本番のファイルで試す前に、コピーしたファイルで試してください（上書き保存するスクリプトの場合は特に）。

## 依頼の例

入力して実行するツール:

```
フォルダの中に支店ごとの売上ファイル（東京.xlsx、大阪.xlsx…）があります。
各ファイルの「売上」シートは 1 行目が見出しで、列は「日付・担当・商品・数量・金額」です。
全ファイルをまとめて、担当ごと・月ごとの金額の合計表を作り、新しい Excel に保存したいです。
```

開いたまま使うアプリ:

```
備品の台帳を Excel の代わりに管理したいです。共有フォルダに置いて総務課の 3 人で使います。
項目は「管理番号・品名・設置場所・購入日・金額・状態（使用中／修理中／廃棄）」です。
開くと一覧が出て、品名や設置場所で絞り込めること。一覧から選んで内容を直したり、新しい備品を登録したりしたいです。
いまの台帳（備品.xlsx の「台帳」シート）を最初に取り込み、一覧は Excel に出力できるようにしてください。
```

## 資料を更新するとき（開発者向け）

- API を変えたら `types/xljs.d.ts` とリファレンスサイトの原稿（`docs/site/`）と合わせて `prompt.md`・`reference.md`・`copilot/instructions.md` も直し、`reference.md` の節を `copilot/skills/xljs-*/SKILL.md` に写す（`XLJS_UPDATE_SKILLS=1 cargo test --lib ai_reference_skills` で、各節をそれを持つスキルへ、新しい節は `xljs-reference` へコピーする。`cargo test --lib ai_reference_skills` が一致を検査する）。`chatgpt/`・`gemini/`・`claude/` の `instructions.md` は `copilot/instructions.md` から作る（同じ `XLJS_UPDATE_SKILLS=1` で `cargo test --lib ai_knowledge_instructions` を流すと書き出し、ふだんは一致を検査する。替え方は `src/engine/tests.rs` の `knowledge_instructions`）。節（`## `）をほかのスキルに移すときは、見出しの行だけをそのスキルに書いてからコピーする。ここに書いたことは AI がそのまま使うので、存在しないオプションや古い書き方を残さない。
- `cargo test --lib ai_docs` がコードブロック（```js）の構文と、`prompt.md` の長さ（貼り付けの上限に収まるよう 15,000 字以内）、`copilot/instructions.md` と ChatGPT・Gemini・Claude 用の指示の長さ（8,000 字以内）、各スキルの長さ（Copilot の上限の 30,000 字以内）を検査する。
- コード例（```` ```js ````）はリファレンスサイトのビルド（`tools/docs/build.mjs`）が d.ts に対して型チェックする（存在しないメソッド・オプションを書くと CI が失敗する。`docs/development.md`）。
- コード例は `xljs --ui=mock --ui-answers=応答.json` で実際に動かして確かめる。
