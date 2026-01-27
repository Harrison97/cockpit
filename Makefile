PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin

.PHONY: build release install uninstall clean

build:
	cargo build

release:
	cargo build --release

install:
	@test -f target/release/cockpit || { echo "Run 'make release' first"; exit 1; }
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 target/release/cockpit $(DESTDIR)$(BINDIR)/cockpit

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/cockpit

clean:
	cargo clean
