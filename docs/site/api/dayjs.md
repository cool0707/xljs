---
title: 日付（Day.js）
api: DayjsStatic, dayjs, Dayjs, DayjsDuration, DayjsInput, DayjsUnit
---

[Day.js](https://day.js.org/) を組み込んでいます（グローバルの `dayjs`。`import` は不要）。エラー・警告と同じ言語（システムの表示言語、`--lang` で変更可）のロケールと主なプラグイン（customParseFormat, isBetween, isSameOrAfter / isSameOrBefore, minMax, quarterOfYear, weekOfYear, isoWeek, dayOfYear, duration, relativeTime, advancedFormat, isLeapYear, isToday など）を読み込み済みです。

```js
const d = dayjs(r.日付);                       // Date・"2026/4/1"・"2026-04-01"（文字列はローカル時刻）
d.format("YYYY年M月D日(ddd)");                // "2026年4月1日(水)"
d.add(1, "month").endOf("month");              // 翌月末
dayjs(b).diff(a, "day");                       // 日数の差
dayjs("20260401", "YYYYMMDD");                 // 形式を指定して読む
dayjs.fromExcel(45383);                        // Excel の日付シリアル値 → 2024/4/1（xljs 拡張）
sheet.cell("A1").value = dayjs();              // dayjs の値はセル・CSV・JSON・表にそのまま渡せる（Date として扱う）
```

- ロケールはエラー・警告と同じ言語です（日本語以外は英語。`dayjs.locale("ja")` / `dayjs.locale("en")` で切り替え）。
- Day.js は MIT ライセンスです（`src/api/dayjs/LICENSE`）。
