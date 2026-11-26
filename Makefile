CARGO ?= cargo
PREFIX ?= /usr/local
DESTDIR ?=
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
ICONDIR ?= $(DATADIR)/icons/hicolor/128x128/apps
LANGUAGEDIR ?= $(DATADIR)/palobar/language-specs
PAMDIR ?= /etc/pam.d
PAM_FLAVOR ?= auto
PALOBAR_BIN = target/release/palobar

.PHONY: all build install install-pam uninstall clean

all: build

build:
	$(CARGO) build --release --locked

install:
	test -x "$(PALOBAR_BIN)" || { echo "Run 'make build' first." >&2; exit 1; }
	install -Dm755 "$(PALOBAR_BIN)" "$(DESTDIR)$(BINDIR)/palobar"
	install -Dm644 kdl.lang "$(DESTDIR)$(LANGUAGEDIR)/kdl.lang"
	install -Dm644 assets/palobar-prefs.png "$(DESTDIR)$(ICONDIR)/io.Astal.palobar-prefs.png"

install-pam:
	@set -eu; \
	flavor="$(PAM_FLAVOR)"; \
	if [ "$$flavor" = auto ]; then \
		. /etc/os-release; \
		flavor="$${ID:-}"; \
	fi; \
	case "$$flavor" in \
		arch) template=packaging/pam/astal-auth.arch ;; \
		debian|ubuntu) template=packaging/pam/astal-auth.debian ;; \
		fedora) template=packaging/pam/astal-auth.fedora ;; \
		*) echo "Unsupported PAM profile '$$flavor'; set PAM_FLAVOR explicitly." >&2; exit 1 ;; \
	esac; \
	destination="$(DESTDIR)$(PAMDIR)/astal-auth"; \
	if [ -e "$$destination" ]; then \
		echo "Refusing to overwrite existing PAM service: $$destination" >&2; \
		exit 1; \
	fi; \
	install -Dm644 "$$template" "$$destination"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/palobar"
	rm -f "$(DESTDIR)$(LANGUAGEDIR)/kdl.lang"
	rm -f "$(DESTDIR)$(ICONDIR)/io.Astal.palobar-prefs.png"

clean:
	$(CARGO) clean