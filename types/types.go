// Package types embeds the TypeScript declarations of the xljs API.
package types

import _ "embed"

// DTS is the content of xljs.d.ts.
//
//go:embed xljs.d.ts
var DTS string
