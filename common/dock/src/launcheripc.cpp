#include "launcheripc.h"
#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QThread>
#include <QTimer>
#include <cstdio>
namespace {
constexpr int maximumMessage = 256 * 1024;
}
LauncherIpc::LauncherIpc(Handler handler, QObject *parent)
    : QObject(parent), handler_(std::move(handler)) {
  server_.setSocketOptions(QLocalServer::UserAccessOption);
  connect(&server_, &QLocalServer::newConnection, this, [this] {
    while (auto *socket = server_.nextPendingConnection())
      acceptSocket(socket);
  });
}
bool LauncherIpc::listen(const QString &name) {
  // Caller holds the single-instance lock, so an existing endpoint is stale.
  QLocalServer::removeServer(name);
  return server_.listen(name);
}
void LauncherIpc::acceptSocket(QLocalSocket *socket) {
  socket->setParent(this);
  socket->setReadBufferSize(maximumMessage + 1);
  auto *timeout = new QTimer(socket);
  timeout->setSingleShot(true);
  timeout->start(3000);
  connect(timeout, &QTimer::timeout, socket, [socket] {
    socket->abort();
    socket->deleteLater();
  });
  connect(socket, &QLocalSocket::disconnected, socket, &QObject::deleteLater);
  connect(socket, &QLocalSocket::readyRead, this, [this, socket, timeout] {
    if (socket->property("handled").toBool())
      return;
    bool ok = false;
    if (socket->bytesAvailable() > maximumMessage) {
      socket->abort();
      return;
    }
    if (!socket->canReadLine())
      return;
    socket->setProperty("handled", true);
    timeout->stop();
    const auto doc =
        QJsonDocument::fromJson(socket->readLine(maximumMessage + 1));
    const auto request = doc.object();
    const auto array = request["urls"].toArray();
    if (request["op"] == "pinFiles" && !array.isEmpty() && array.size() <= 32) {
      QStringList urls;
      bool strings = true;
      for (const auto &v : array) {
        strings &= v.isString();
        urls.append(v.toString());
      }
      if (strings)
        ok = handler_(urls);
    }
    socket->write(
        QJsonDocument(QJsonObject{{"ok", ok}}).toJson(QJsonDocument::Compact) +
        '\n');
    socket->disconnectFromServer();
  });
}
int LauncherIpc::submit(const QString &name, const QStringList &urls) {
  QLocalSocket socket;
  QElapsedTimer elapsed;
  elapsed.start();
  while (elapsed.elapsed() < 3000) {
    socket.connectToServer(name);
    if (socket.waitForConnected(200))
      break;
    socket.abort();
    QThread::msleep(50);
  }
  if (socket.state() != QLocalSocket::ConnectedState) {
    std::fprintf(
        stderr,
        "Cannot contact the running Dock. Restart it after updating.\n");
    return 1;
  }
  const auto bytes =
      QJsonDocument(QJsonObject{{"op", "pinFiles"},
                                {"urls", QJsonArray::fromStringList(urls)}})
          .toJson(QJsonDocument::Compact) +
      '\n';
  if (bytes.size() > maximumMessage || socket.write(bytes) != bytes.size() ||
      (socket.bytesToWrite() > 0 && !socket.waitForBytesWritten(3000)))
    return 1;
  elapsed.restart();
  while (!socket.canReadLine() && elapsed.elapsed() < 4000) {
    if (!socket.waitForReadyRead(qMax(1, 4000 - int(elapsed.elapsed()))))
      break;
  }
  if (!QJsonDocument::fromJson(socket.readLine()).object()["ok"].toBool()) {
    std::fprintf(stderr, "Dock could not add the launcher. Check its "
                         "notification or terminal output.\n");
    return 1;
  }
  return 0;
}
