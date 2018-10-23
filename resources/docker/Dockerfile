FROM fedora
# copy operon binary
COPY ./op ./opt/operon/op

# copy operon config to $HOME
COPY ./.operon /root/.operon

# enable rust backtraces
ENV RUST_BACKTRACE=1

# enable logging for op-net
#ENV RUST_LOG="op_net=debug"

# add operon executable to path
ENV PATH="${PATH}:/opt/operon/"

# install some network tools for convenience
#RUN dnf install -y iproute
#RUN dnf install -y net-tools

EXPOSE 6666/tcp
EXPOSE 6666/udp
# start operon service
CMD ["op","start","--pid-path","/var/run/operon/op.pid","--do-not-daemonize"]
