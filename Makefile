PANDOC  ?= pandoc
PREFIX  ?= /usr/local
DESTDIR ?=

BINDIR  = $(DESTDIR)$(PREFIX)/bin
MANDIR  = $(DESTDIR)$(PREFIX)/share/man/man1

BINARY  = target/release/mergelog-rs
MDSRC   = docs/mergelog-rs.md
MANPAGE = docs/mergelog-rs.1

.PHONY: all build man install clean

all: build man

build:
	cargo build --release --locked

man: $(MANPAGE)

$(MANPAGE): $(MDSRC)
	$(PANDOC) --standalone --to man $< --output $@

install: build man
	install -Dm755 $(BINARY)  $(BINDIR)/mergelog-rs
	install -Dm644 $(MANPAGE) $(MANDIR)/mergelog-rs.1

clean:
	cargo clean
