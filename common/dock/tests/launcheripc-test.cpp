#include "launcheripc.h"
#include <QCoreApplication>
#include <QElapsedTimer>
#include <QJsonDocument>
#include <QJsonObject>
#include <QThread>
#include <cstdio>
#include <sys/socket.h>
#define CHECK(v)                                                               \
  do {                                                                         \
    if (!(v)) {                                                                \
      std::fprintf(stderr, "IPC test failed at %d: %s\n", __LINE__, #v);       \
      return 1;                                                                \
    }                                                                          \
  } while (false)
static bool waitFor(const std::function<bool()> &check) {
  QElapsedTimer t;
  t.start();
  while (!check() && t.elapsed() < 1500) {
    QCoreApplication::processEvents();
    QThread::msleep(2);
  }
  return check();
}
int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  int calls = 0;
  QStringList received;
  LauncherIpc ipc([&](const QStringList &urls) {
    ++calls;
    received = urls;
    return urls.first().endsWith(".desktop");
  });
  for (const auto &request : QList<QByteArray>{
           "{\"op\":\"pinFiles\",\"urls\":[\"file:///tmp/"
           "app%20name.desktop\"]}",
           "{\"op\":\"pinFiles\",\"urls\":[]}",
           "{\"op\":\"other\",\"urls\":[\"file:///tmp/a.desktop\"]}",
           "{\"op\":\"pinFiles\",\"urls\":[12]}",
           "{\"op\":\"pinFiles\",\"urls\":[\"file:///tmp/a.txt\"]}"}) {
    int fds[2];
    CHECK(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, fds) == 0);
    QLocalSocket client;
    CHECK(client.setSocketDescriptor(fds[0]));
    auto *server = new QLocalSocket;
    CHECK(server->setSocketDescriptor(fds[1]));
    ipc.acceptSocket(server);
    const auto before = calls;
    CHECK(client.write(request) == request.size());
    client.flush();
    QCoreApplication::processEvents();
    CHECK(calls == before); // No newline: no dispatch yet.
    client.write("\n");
    client.flush();
    CHECK(waitFor([&] { return client.canReadLine(); }));
    const bool ok =
        QJsonDocument::fromJson(client.readLine()).object()["ok"].toBool();
    CHECK(ok == (request.contains("app%20name.desktop")));
  }
  CHECK(calls == 2);
  CHECK(received == QStringList{"file:///tmp/a.txt"});
  // Test the real listener too when this environment permits local socket
  // binding.
  const auto name =
      QString("/tmp/dock-ipc-test-%1").arg(QCoreApplication::applicationPid());
  if (ipc.listen(name)) {
    QLocalSocket client;
    client.connectToServer(name);
    CHECK(client.waitForConnected(500));
    client.write(
        "{\"op\":\"pinFiles\",\"urls\":[\"file:///tmp/live.desktop\"]}\n");
    client.flush();
    CHECK(waitFor([&] { return client.canReadLine(); }));
    CHECK(QJsonDocument::fromJson(client.readLine()).object()["ok"].toBool());
    std::puts("PASS: real local listener");
  } else
    std::puts("Local socket binding unavailable here; socket-pair protocol "
              "checks passed.");
  std::puts("PASS: framed requests, incomplete messages, validation, handler "
            "dispatch and response");
  return 0;
}
