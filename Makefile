# berm Makefile

# Run the workbench with its icon. `cargo run -p berm-gui` is faster to type and
# builds the same binary, but macOS reads an app's icon out of its bundle and
# nowhere else, so a bare one comes up generic.
app: quit
	./apps/gui/bundle.sh debug
	open target/berm.app

# The shipping berm.app, printed rather than opened — it is an artifact to
# install, and `make app` is how you look at one.
bundle:
	./apps/gui/bundle.sh release

# Take down whatever berm holds :7777, bundled or bare. `open` activates a
# running instance rather than launching the build just made, so without this
# `make app` would rebuild and then hand you back the old process.
quit:
	@pids=$$(pgrep -f 'berm\.app/Contents/MacOS/berm|target/[a-z]+/berm-gui'); \
	  [ -z "$$pids" ] || { echo "stopping berm ($$pids)"; kill $$pids; sleep 1; }

.PHONY: app bundle quit
