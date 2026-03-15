pandoc := env_var_or_default("PANDOC", "pandoc")
prefix := env_var_or_default("PREFIX", "/usr/local")
destdir := env_var_or_default("DESTDIR", "")

bindir := destdir + prefix + "/bin"
mandir := destdir + prefix + "/share/man/man1"

binary  := "target/release/mergelog-rs"
mdsrc   := "docs/mergelog-rs.md"
manpage := "docs/mergelog-rs.1"

# Build binary and man page
default: build man

# Compile release binary
build:
    cargo build --release --locked

# Generate man page from Markdown via pandoc
man:
    {{pandoc}} --standalone --to man {{mdsrc}} --output {{manpage}}

# Install binary and man page
install: build man
    install -Dm755 {{binary}}  {{bindir}}/mergelog-rs
    install -Dm644 {{manpage}} {{mandir}}/mergelog-rs.1

# Remove build artifacts
clean:
    cargo clean
