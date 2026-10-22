package i18n

import (
	"errors"
	"io/fs"
	"os"
	"regexp"
	"runtime"
	"strings"
	"syscall"
)

type rule struct {
	re   *regexp.Regexp
	repl string
}

func r(pattern, repl string) rule { return rule{regexp.MustCompile(pattern), repl} }

// rules translate well-known English messages into Japanese. They are
// applied in order to the whole text, so more specific patterns come first.
var rules = []rule{
	// goja (runtime errors)
	r(`Cannot read property '([^']*)' of (undefined|null)`, "$2 のプロパティ '$1' は読み取れません"),
	r(`Cannot set property '([^']*)' of (undefined|null)`, "$2 にプロパティ '$1' は設定できません"),
	r(`Object has no member '([^']*)'`, "オブジェクトにメンバー '$1' はありません（関数名・プロパティ名を確認してください）"),
	r(`([\w$.\[\]'"]+) is not a function`, "$1 は関数ではありません"),
	r(`([\w$.\[\]'"]+) is not a constructor`, "$1 はコンストラクタではありません"),
	r(`([\w$.\[\]'"]+) is not iterable`, "$1 は反復可能ではありません"),
	r(`([\w$]+) is not defined`, "$1 は定義されていません"),
	r(`Assignment to constant variable\.?`, "const で宣言した変数には再代入できません"),
	r(`Cannot access '([^']*)' before initialization`, "初期化前の '$1' にはアクセスできません"),
	r(`Maximum call stack size exceeded`, "呼び出しが深すぎます（無限再帰の可能性があります）"),
	r(`Invalid array length`, "配列の長さが不正です"),
	r(`Cannot convert undefined or null to object`, "undefined / null はオブジェクトに変換できません"),
	r(`Cannot convert a Symbol value to a string`, "Symbol は文字列に変換できません"),
	r(`Cannot mix BigInt and other types.*`, "BigInt と他の型は混ぜて計算できません"),
	r(`invalid character '([^']*)' looking for beginning of value`, "JSON の解析に失敗しました（予期しない文字 '$1'）"),
	r(`invalid character '([^']*)' after (object key|object key:value pair|array element)`, "JSON の解析に失敗しました（予期しない文字 '$1'）"),
	r(`unexpected end of JSON input`, "JSON の解析に失敗しました（途中で終わっています）"),
	r(`Unhandled promise rejection: `, "処理されなかった Promise の reject: "),

	// esbuild (syntax / resolution errors)
	r(`Could not resolve "([^"]*)"`, `"$1" が見つかりません（import のパスを確認してください）`),
	r(`Expected "([^"]*)" but found end of file`, `"$1" が必要ですが、ファイルが終わっています（括弧の閉じ忘れなど）`),
	r(`Expected "([^"]*)" but found "([^"]*)"`, `"$1" が必要ですが "$2" があります`),
	r(`Expected identifier but found "([^"]*)"`, `識別子が必要ですが "$1" があります`),
	r(`Expected (.+?) but found "([^"]*)"`, `$1 が必要ですが "$2" があります`),
	r(`Unexpected end of file`, "ファイルが途中で終わっています（括弧の閉じ忘れなど）"),
	r(`Unexpected "([^"]*)"`, `予期しない "$1" があります`),
	r(`Unterminated string literal`, "文字列が閉じられていません"),
	r(`Unterminated template literal`, "テンプレート文字列が閉じられていません"),
	r(`Unterminated regular expression`, "正規表現が閉じられていません"),
	r(`The symbol "([^"]*)" has already been declared`, `"$1" は既に宣言されています`),
	r(`Top-level return cannot be used.*`, "トップレベルでは return を使えません（exit() を使ってください）"),
	r(`Cannot use "await" outside.*`, `async 関数の外では "await" を使えません`),

	// excelize / zip
	r(`zip: not a valid zip file`, "xlsx として読み込めません（形式が違うか、ファイルが壊れています）"),
	r(`sheet (.+?) does not exist`, `シート "$1" はありません`),
	r(`cannot convert cell "([^"]*)" to coordinates: invalid cell name "[^"]*"`, `セル番地 "$1" が不正です`),
	r(`invalid cell name "([^"]*)"`, `セル番地 "$1" が不正です`),
	r(`invalid column name "([^"]*)"`, `列名 "$1" が不正です`),
	r(`the sheet name length exceeds the (\d+) characters limit`, "シート名は $1 文字までです"),
	r(`the sheet can not contain any of the characters.*`, `シート名に使えない文字（: \ / ? * [ ]）が含まれています`),
	r(`the sheet name can not be blank`, "シート名を空にはできません"),
	r(`the encrypted file is not supported.*|unsupported encryption mechanism`, "パスワード付きのファイルには対応していません"),
}

// Text translates known English messages in s when Japanese is selected.
func Text(s string) string {
	if !Japanese() || s == "" {
		return s
	}
	for _, rl := range rules {
		s = rl.re.ReplaceAllString(s, rl.repl)
	}
	return s
}

var opNames = map[string][2]string{
	"open":     {"開けません", "cannot open"},
	"read":     {"読み込めません", "cannot read"},
	"write":    {"書き込めません", "cannot write"},
	"close":    {"閉じられません", "cannot close"},
	"remove":   {"削除できません", "cannot remove"},
	"unlinkat": {"削除できません", "cannot remove"},
	"mkdir":    {"フォルダを作成できません", "cannot create folder"},
	"stat":     {"情報を取得できません", "cannot access"},
	"lstat":    {"情報を取得できません", "cannot access"},
	"readdir":  {"フォルダを読み込めません", "cannot read folder"},
	"chdir":    {"フォルダに移動できません", "cannot change directory to"},
	"chtimes":  {"日時を設定できません", "cannot set times of"},
	"rename":   {"名前を変更・移動できません", "cannot rename"},
	"symlink":  {"リンクを作成できません", "cannot create link"},
}

// reason describes an OS-level error cause in Japanese.
func reason(err error) string {
	var errno syscall.Errno
	if runtime.GOOS == "windows" && errors.As(err, &errno) {
		switch errno {
		case 32, 33: // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
			return "他のプログラム（Excel など）で開かれています。閉じてから再実行してください"
		}
	}
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return "ファイルまたはフォルダが見つかりません"
	case errors.Is(err, fs.ErrPermission):
		return "アクセスが拒否されました（読み取り専用、権限、または他のプログラムが使用中）"
	case errors.Is(err, fs.ErrExist):
		return "既に存在します"
	}
	if runtime.GOOS != "windows" && errors.As(err, &errno) {
		switch errno {
		case syscall.ENOTDIR:
			return "フォルダではありません"
		case syscall.EISDIR:
			return "フォルダです"
		case syscall.ENOTEMPTY:
			return "フォルダが空ではありません"
		case syscall.ENOSPC:
			return "ディスクの空き容量が足りません"
		}
	}
	// Windows error texts come from the system and are already localized.
	return Text(err.Error())
}

// Error returns the user-facing message for err.
func Error(err error) string {
	if err == nil {
		return ""
	}
	var le *os.LinkError
	if errors.As(err, &le) {
		if !Japanese() {
			return err.Error()
		}
		return le.Old + " → " + le.New + ": " + opName(le.Op) + ": " + reason(le.Err)
	}
	var pe *fs.PathError
	if errors.As(err, &pe) {
		if !Japanese() {
			return err.Error()
		}
		msg := pe.Path + ": " + opName(pe.Op) + ": " + reason(pe.Err)
		// Keep any context added around the path error.
		if full := err.Error(); full != pe.Error() {
			if i := strings.Index(full, pe.Error()); i > 0 {
				msg = Text(full[:i]) + msg
			}
		}
		return msg
	}
	return Text(err.Error())
}

func opName(op string) string {
	if n, ok := opNames[op]; ok {
		return S(n[0], n[1])
	}
	return op
}
