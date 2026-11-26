CARGO ?= cargo
PREFIX ?= /usr/local
DESTDIR ?=
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
ICONDIR ?= $(DATADIR)/icons/hicolor/128x128/apps
LANGUAGEDIR ?= $(DATADIR)/palobar/language-specs
PALOBAR_BIN = target/release/palobar

.PHONY: all build install uninstall clean

all: build

build:
	$(CARGO) build --release --locked

install:
	test -x "$(PALOBAR_BIN)" || { echo "Run 'make build' first." >&2; exit 1; }
	install -Dm755 "$(PALOBAR_BIN)" "$(DESTDIR)$(BINDIR)/palobar"
	install -Dm644 kdl.lang "$(DESTDIR)$(LANGUAGEDIR)/kdl.lang"
	install -Dm644 assets/palobar-prefs.png "$(DESTDIR)$(ICONDIR)/io.Astal.palobar-prefs.png"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/palobar"
	rm -f "$(DESTDIR)$(LANGUAGEDIR)/kdl.lang"
	rm -f "$(DESTDIR)$(ICONDIR)/io.Astal.palobar-prefs.png"

clean:
	$(CARGO) clean