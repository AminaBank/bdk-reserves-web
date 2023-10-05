TAG := bdk-reserves-web

run: builder
	docker run --rm --tty -e PORT='8888' -p 8888:8888 ${TAG}

builder:
	docker build --tag ${TAG} .
