# make && sudo make install          (PREFIX=/usr/local by default; DESTDIR for packagers)
PREFIX ?= /usr/local

all:
	cargo build --release --locked

install: all
	install -Dm755 target/release/frost $(DESTDIR)$(PREFIX)/bin/frost
	install -Dm644 frost.1 $(DESTDIR)$(PREFIX)/share/man/man1/frost.1
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/frost/LICENSE

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/frost $(DESTDIR)$(PREFIX)/share/man/man1/frost.1

test:
	./test

.PHONY: all install uninstall test
