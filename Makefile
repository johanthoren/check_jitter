export DOCKER_BUILDKIT=1

.PHONY: all linux windows clean

all: linux windows

linux:
	@docker build --platform=linux/amd64 --target bin -f ./Dockerfile.linux --output . .

windows:
	@docker build --target bin -f ./Dockerfile.windows --output . .

clean:
	rm -rf target
	rm -f check_jitter-x86_64-unknown-linux-musl
	rm -f check_jitter-x86_64-pc-windows-gnu.exe
	rm -f check-jitter*.rpm
	rm -f check-jitter*.deb
	rm -f check_jitter
	rm -f check-jitter
