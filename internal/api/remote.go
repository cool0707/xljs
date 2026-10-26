package api

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"math/big"
	"strconv"
	"time"

	"github.com/dop251/goja"
)

// Remote serves the natives to a script that runs in another JavaScript
// engine: the debugger page (internal/ui/web/debug.js), where the prelude
// and the script run in WebView2 so that its developer tools can debug them.
// Calls arrive as JSON; values travel in this wire format:
//
//	null, booleans, numbers, strings, arrays  as in JSON
//	{"$o": {...}}                             plain object (keys in order)
//	{"$u": 0}                                 undefined
//	{"$num": "NaN"}                           NaN, Infinity, -Infinity, -0
//	{"$big": "123"}                           bigint
//	{"$date": ms}                             Date (null = invalid date)
//	{"$bytes": base64, "$t": "Uint8Array"}    ArrayBuffer or typed array
//	{"$err": {"name", "message", "stack"}}    Error
//	{"$fn": id}                               function of the page
//	{"$ref": id, "$k": "fn"|"obj"}            function or object of this side
//	{"$p": id}                                promise of this side
//
// Functions of the page are called with an event {"ev": "call", "fn", "args"}
// (without waiting for them); promises of this side are settled with
// {"ev": "settle", "p", "ok", "value"}. Objects of this side (workbooks,
// windows) stay here and are referred to by id. A Remote must only be used
// on the script thread.
type Remote struct {
	vm   *goja.Runtime
	n    *natives
	refs map[int64]goja.Value
	ids  map[*goja.Object]int64
	next int64
	sent int
	// Send delivers an event to the page. Set it before the first call.
	Send func(event string)
}

// NewRemote defines the natives in vm (without the prelude, which runs in
// the page).
func NewRemote(vm *goja.Runtime, env *Env) *Remote {
	return &Remote{vm: vm, n: installNatives(vm, env), refs: map[int64]goja.Value{}, ids: map[*goja.Object]int64{}}
}

// Sent returns the number of events sent to the page so far.
func (r *Remote) Sent() int { return r.sent }

func (r *Remote) send(event map[string]any) {
	b, err := json.Marshal(event)
	if err != nil {
		panic(err)
	}
	r.sent++
	r.Send(string(b))
}

// Manifest returns the natives object in the wire format: its functions
// become references the page can call.
func (r *Remote) Manifest() json.RawMessage {
	return r.encode(r.n.obj)
}

// Call calls the native name. The result is the response to the page:
// {"value": v} or {"error": e}.
func (r *Remote) Call(name string, args []json.RawMessage) []byte {
	fn, ok := goja.AssertFunction(r.n.obj.Get(name))
	if !ok {
		return r.fail(fmt.Errorf("unknown native %q", name))
	}
	return r.call(fn, goja.Undefined(), args)
}

// Invoke calls the function with reference id (see the wire format).
func (r *Remote) Invoke(id int64, args []json.RawMessage) []byte {
	fn, ok := goja.AssertFunction(r.refs[id])
	if !ok {
		return r.fail(fmt.Errorf("unknown function reference %d", id))
	}
	return r.call(fn, goja.Undefined(), args)
}

func (r *Remote) call(fn goja.Callable, this goja.Value, args []json.RawMessage) []byte {
	vals := make([]goja.Value, len(args))
	for i, a := range args {
		v, err := r.Decode(a)
		if err != nil {
			return r.fail(err)
		}
		vals[i] = v
	}
	res, err := fn(this, vals...)
	if err != nil {
		return r.fail(err)
	}
	return response("value", r.encode(res))
}

// fail answers with err: a JavaScript exception keeps its value, other
// errors become an Error.
func (r *Remote) fail(err error) []byte {
	var ex *goja.Exception
	if errors.As(err, &ex) {
		return response("error", r.encode(ex.Value()))
	}
	return response("error", errValue("Error", err.Error(), ""))
}

func response(key string, v json.RawMessage) []byte {
	var b bytes.Buffer
	b.WriteString(`{"` + key + `":`)
	b.Write(v)
	b.WriteString("}")
	return b.Bytes()
}

func errValue(name, message, stack string) json.RawMessage {
	b, _ := json.Marshal(map[string]any{"$err": map[string]string{"name": name, "message": message, "stack": stack}})
	return b
}

// ref returns the id of a value of this side, registering it.
func (r *Remote) ref(obj *goja.Object) int64 {
	if id, ok := r.ids[obj]; ok {
		return id
	}
	r.next++
	r.refs[r.next] = obj
	r.ids[obj] = r.next
	return r.next
}

func (r *Remote) encode(v goja.Value) json.RawMessage {
	var b bytes.Buffer
	r.encodeTo(&b, v, 0)
	return b.Bytes()
}

func writeJSON(b *bytes.Buffer, v any) {
	data, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	b.Write(data)
}

func (r *Remote) encodeTo(b *bytes.Buffer, v goja.Value, depth int) {
	if depth > 100 {
		panic(r.vm.NewTypeError("value is nested too deeply (circular?)"))
	}
	switch {
	case v == nil || goja.IsUndefined(v):
		b.WriteString(`{"$u":0}`)
		return
	case goja.IsNull(v):
		b.WriteString("null")
		return
	}
	obj, isObj := v.(*goja.Object)
	if !isObj {
		switch x := v.Export().(type) {
		case bool, string:
			writeJSON(b, x)
		case int64:
			b.WriteString(strconv.FormatInt(x, 10))
		case float64:
			writeNumber(b, x)
		case *big.Int:
			writeJSON(b, map[string]string{"$big": x.String()})
		default:
			b.WriteString(`{"$u":0}`) // symbols
		}
		return
	}
	if _, ok := goja.AssertFunction(obj); ok {
		fmt.Fprintf(b, `{"$ref":%d,"$k":"fn"}`, r.ref(obj))
		return
	}
	switch obj.ClassName() {
	case "Array":
		b.WriteString("[")
		l := int(obj.Get("length").ToInteger())
		for i := 0; i < l; i++ {
			if i > 0 {
				b.WriteString(",")
			}
			r.encodeTo(b, obj.Get(strconv.Itoa(i)), depth+1)
		}
		b.WriteString("]")
		return
	case "Date":
		if t, ok := obj.Export().(time.Time); ok {
			writeJSON(b, map[string]int64{"$date": t.UnixMilli()})
		} else {
			b.WriteString(`{"$date":null}`)
		}
		return
	case "Error":
		b.Write(errValue(strProp(obj, "name"), strProp(obj, "message"), strProp(obj, "stack")))
		return
	}
	switch x := obj.Export().(type) {
	case *goja.Promise:
		r.watch(obj)
		fmt.Fprintf(b, `{"$p":%d}`, r.ref(obj))
	case goja.ArrayBuffer:
		writeJSON(b, map[string]string{"$bytes": base64.StdEncoding.EncodeToString(x.Bytes()), "$t": "ArrayBuffer"})
	case map[string]any:
		b.WriteString(`{"$o":{`)
		for i, k := range obj.Keys() {
			if i > 0 {
				b.WriteString(",")
			}
			writeJSON(b, k)
			b.WriteString(":")
			r.encodeTo(b, obj.Get(k), depth+1)
		}
		b.WriteString("}}")
	default:
		if t := typedArrayName(obj); t != "" {
			data := r.n.bytes(obj)
			writeJSON(b, map[string]string{"$bytes": base64.StdEncoding.EncodeToString(data), "$t": t})
			return
		}
		fmt.Fprintf(b, `{"$ref":%d,"$k":"obj"}`, r.ref(obj))
	}
}

func writeNumber(b *bytes.Buffer, f float64) {
	switch {
	case math.IsNaN(f):
		b.WriteString(`{"$num":"NaN"}`)
	case math.IsInf(f, 1):
		b.WriteString(`{"$num":"Infinity"}`)
	case math.IsInf(f, -1):
		b.WriteString(`{"$num":"-Infinity"}`)
	case f == 0 && math.Signbit(f):
		b.WriteString(`{"$num":"-0"}`)
	default:
		b.WriteString(strconv.FormatFloat(f, 'g', -1, 64))
	}
}

func strProp(obj *goja.Object, name string) string {
	v := obj.Get(name)
	if isNullish(v) {
		return ""
	}
	return v.String()
}

var typedArrays = map[string]bool{
	"Int8Array": true, "Uint8Array": true, "Uint8ClampedArray": true, "Int16Array": true, "Uint16Array": true,
	"Int32Array": true, "Uint32Array": true, "Float32Array": true, "Float64Array": true,
	"BigInt64Array": true, "BigUint64Array": true, "DataView": true,
}

// typedArrayName returns the constructor name of a typed array or DataView.
func typedArrayName(obj *goja.Object) string {
	c, ok := obj.Get("constructor").(*goja.Object)
	if !ok {
		return ""
	}
	name := strProp(c, "name")
	if typedArrays[name] {
		if _, ok := obj.Get("buffer").Export().(goja.ArrayBuffer); ok {
			return name
		}
	}
	return ""
}

// watch tells the page when the promise settles.
func (r *Remote) watch(p *goja.Object) {
	if _, ok := r.ids[p]; ok {
		return
	}
	then, ok := goja.AssertFunction(p.Get("then"))
	if !ok {
		return
	}
	settle := func(ok bool) goja.Value {
		return r.vm.ToValue(func(c goja.FunctionCall) goja.Value {
			r.send(map[string]any{"ev": "settle", "p": r.ids[p], "ok": ok, "value": r.encode(c.Argument(0))})
			return goja.Undefined()
		})
	}
	// ref first, so that the id exists when an already settled promise
	// runs its reaction.
	r.ref(p)
	if _, err := then(p, settle(true), settle(false)); err != nil {
		panic(err)
	}
}

// Decode converts a value in the wire format.
func (r *Remote) Decode(data json.RawMessage) (goja.Value, error) {
	d := json.NewDecoder(bytes.NewReader(data))
	d.UseNumber()
	return r.decode(d)
}

func (r *Remote) decode(d *json.Decoder) (goja.Value, error) {
	tok, err := d.Token()
	if err != nil {
		return nil, err
	}
	switch x := tok.(type) {
	case nil:
		return goja.Null(), nil
	case bool:
		return r.vm.ToValue(x), nil
	case string:
		return r.vm.ToValue(x), nil
	case json.Number:
		if i, err := x.Int64(); err == nil {
			return r.vm.ToValue(i), nil
		}
		f, err := x.Float64()
		return r.vm.ToValue(f), err
	case json.Delim:
		if x == '[' {
			var items []any
			for d.More() {
				v, err := r.decode(d)
				if err != nil {
					return nil, err
				}
				items = append(items, v)
			}
			_, err := d.Token() // ]
			return r.vm.NewArray(items...), err
		}
		return r.decodeObject(d)
	}
	return nil, fmt.Errorf("unexpected token %v", tok)
}

// decodeObject decodes a tagged object; the opening brace is consumed.
func (r *Remote) decodeObject(d *json.Decoder) (goja.Value, error) {
	var v goja.Value
	tags := map[string]json.RawMessage{}
	for d.More() {
		tok, err := d.Token()
		if err != nil {
			return nil, err
		}
		key, _ := tok.(string)
		if key == "$o" {
			if v, err = r.decodePlain(d); err != nil {
				return nil, err
			}
			continue
		}
		var raw json.RawMessage
		if err := d.Decode(&raw); err != nil {
			return nil, err
		}
		tags[key] = raw
	}
	if _, err := d.Token(); err != nil { // }
		return nil, err
	}
	if v != nil {
		return v, nil
	}
	str := func(key string) string {
		var s string
		json.Unmarshal(tags[key], &s)
		return s
	}
	num := func(key string) int64 {
		var n int64
		json.Unmarshal(tags[key], &n)
		return n
	}
	switch {
	case tags["$u"] != nil:
		return goja.Undefined(), nil
	case tags["$num"] != nil:
		f, err := strconv.ParseFloat(str("$num"), 64) // keeps -0
		return r.vm.ToValue(f), err
	case tags["$big"] != nil:
		return r.vm.RunString("BigInt(" + strconv.Quote(str("$big")) + ")")
	case tags["$date"] != nil:
		var ms *float64
		json.Unmarshal(tags["$date"], &ms)
		t := math.NaN()
		if ms != nil {
			t = *ms
		}
		return r.construct("Date", r.vm.ToValue(t))
	case tags["$bytes"] != nil:
		data, err := base64.StdEncoding.DecodeString(str("$bytes"))
		if err != nil {
			return nil, err
		}
		buf := r.vm.ToValue(r.vm.NewArrayBuffer(data))
		if t := str("$t"); t != "ArrayBuffer" {
			if !typedArrays[t] {
				return nil, fmt.Errorf("unknown typed array %q", t)
			}
			return r.construct(t, buf)
		}
		return buf, nil
	case tags["$err"] != nil:
		var e struct{ Name, Message, Stack string }
		json.Unmarshal(tags["$err"], &e)
		ctor := e.Name
		if !isErrorCtor(ctor) {
			ctor = "Error"
		}
		obj, err := r.construct(ctor, r.vm.ToValue(e.Message))
		if err == nil {
			o := obj.(*goja.Object)
			if e.Name != ctor {
				o.DefineDataProperty("name", r.vm.ToValue(e.Name), goja.FLAG_TRUE, goja.FLAG_FALSE, goja.FLAG_TRUE)
			}
			if e.Stack != "" {
				o.DefineDataProperty("stack", r.vm.ToValue(e.Stack), goja.FLAG_TRUE, goja.FLAG_FALSE, goja.FLAG_TRUE)
			}
		}
		return obj, err
	case tags["$fn"] != nil:
		id := num("$fn")
		return r.vm.ToValue(func(c goja.FunctionCall) goja.Value {
			args := make([]json.RawMessage, len(c.Arguments))
			for i, a := range c.Arguments {
				args[i] = r.encode(a)
			}
			r.send(map[string]any{"ev": "call", "fn": id, "args": args})
			return goja.Undefined()
		}), nil
	case tags["$ref"] != nil, tags["$p"] != nil:
		id := num("$ref")
		if tags["$p"] != nil {
			id = num("$p")
		}
		if v, ok := r.refs[id]; ok {
			return v, nil
		}
		return nil, fmt.Errorf("unknown reference %d", id)
	}
	return nil, errors.New("untagged object in the wire format")
}

func isErrorCtor(name string) bool {
	switch name {
	case "Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError":
		return true
	}
	return false
}

// decodePlain decodes the members of a {"$o": {...}} object.
func (r *Remote) decodePlain(d *json.Decoder) (goja.Value, error) {
	if tok, err := d.Token(); err != nil || tok != json.Delim('{') {
		if err == nil {
			err = errors.New(`"$o" is not an object`)
		}
		return nil, err
	}
	obj := r.vm.NewObject()
	for d.More() {
		tok, err := d.Token()
		if err != nil {
			return nil, err
		}
		v, err := r.decode(d)
		if err != nil {
			return nil, err
		}
		if err := obj.Set(tok.(string), v); err != nil {
			return nil, err
		}
	}
	_, err := d.Token() // }
	return obj, err
}

func (r *Remote) construct(name string, args ...goja.Value) (goja.Value, error) {
	ctor, ok := goja.AssertConstructor(r.vm.Get(name))
	if !ok {
		return nil, fmt.Errorf("%s is not a constructor", name)
	}
	return ctor(nil, args...)
}
