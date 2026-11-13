# Uses ERBetterLod's toolchain, so there's only one copy of it. Run `make toolchain` there first.
TOOLCHAIN    ?= $(HOME)/workdir/elden/ERBetterLod/.toolchain
export RUSTUP_HOME := $(TOOLCHAIN)/rustup
export CARGO_HOME  := $(TOOLCHAIN)/cargo
export PATH  := $(TOOLCHAIN)/mingw/usr/bin:$(PATH)
CARGO        := $(CARGO_HOME)/bin/cargo
DLL          := target/x86_64-pc-windows-gnu/release/MultipleAutomaticShoutouts.dll
PLUGINS_DIR  ?= $(HOME)/.local/share/Steam/steamapps/common/Monster Hunter World/nativePC/plugins

.PHONY: all build install uninstall clean

all: build

build:
	$(CARGO) build --release

# Doesn't overwrite a config you've already edited.
install: build
	cp $(DLL) "$(PLUGINS_DIR)/"
	cp -n MultipleAutomaticShoutouts.toml "$(PLUGINS_DIR)/" || true

uninstall:
	rm -f "$(PLUGINS_DIR)/MultipleAutomaticShoutouts.dll" "$(PLUGINS_DIR)/MultipleAutomaticShoutouts.toml" "$(PLUGINS_DIR)/MultipleAutomaticShoutouts.log"
	@echo "Kept $(PLUGINS_DIR)/MultipleAutomaticShoutouts.sets.toml, your pages. Delete it by hand if you want them gone."

clean:
	rm -rf target
