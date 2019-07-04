FROM fedora
# copy opereon binary
COPY ./op ./opt/opereon/op

# copy opereon config to $HOME
COPY ./.opereon /root/.opereon

# enable rust backtraces
ENV RUST_BACKTRACE=1

# enable logging for op-net
#ENV RUST_LOG="op_net=debug"

# add opereon executable to path
ENV PATH="${PATH}:/opt/opereon/"

# install some network tools for convenience
#RUN dnf install -y iproute
#RUN dnf install -y net-tools

EXPOSE 6666/tcp
EXPOSE 6666/udp
# start opereon service
CMD ["op","start","--pid-path","/var/run/opereon/op.pid","--do-not-daemonize"]
