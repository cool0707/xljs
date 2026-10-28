VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
LDFLAGS := -s -w -X main.version=$(VERSION)
GO ?= go

.PHONY: all test vet windows windows-x64 windows-arm64 dist linux clean

all: test windows

test:
	$(GO) test ./...

vet:
	$(GO) vet ./...
	GOOS=windows $(GO) vet ./...

# Go calls x64 "amd64".
windows: windows-x64

windows-x64:
	GOOS=windows GOARCH=amd64 $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o dist/x64/xljsc.exe ./cmd/xljs
	GOOS=windows GOARCH=amd64 $(GO) build -trimpath -ldflags "$(LDFLAGS) -H=windowsgui" -o dist/x64/xljs.exe ./cmd/xljs

windows-arm64:
	GOOS=windows GOARCH=arm64 $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o dist/arm64/xljsc.exe ./cmd/xljs
	GOOS=windows GOARCH=arm64 $(GO) build -trimpath -ldflags "$(LDFLAGS) -H=windowsgui" -o dist/arm64/xljs.exe ./cmd/xljs

# Release archives: dist/xljs-windows-x64.zip, dist/xljs-windows-arm64.zip
# (the executables and docs/ai)
dist: windows-x64 windows-arm64
	cd dist && rm -f xljs-windows-*.zip \
	  && zip -j xljs-windows-x64.zip x64/xljs.exe x64/xljsc.exe \
	  && zip -j xljs-windows-arm64.zip arm64/xljs.exe arm64/xljsc.exe
	# Documents for writing scripts with AI chats (docs/ai)
	rm -rf dist/docs && mkdir -p dist/docs && cp -r docs/ai dist/docs/
	cd dist && zip -r xljs-windows-x64.zip docs && zip -r xljs-windows-arm64.zip docs

linux:
	$(GO) build -trimpath -ldflags "$(LDFLAGS)" -o dist/xljs ./cmd/xljs

clean:
	rm -rf dist
