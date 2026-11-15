#pragma once
#include <QLocalServer>
#include <QLocalSocket>
#include <QStringList>
#include <functional>
// Local, current-user-only transport. Import validation remains in Rust.
class LauncherIpc : public QObject {
public:
  using Handler = std::function<bool(const QStringList &)>;
  explicit LauncherIpc(Handler handler, QObject *parent = nullptr);
  bool listen(const QString &name);
  static int submit(const QString &name, const QStringList &urls);
  void acceptSocket(QLocalSocket *socket);

private:
  QLocalServer server_;
  Handler handler_;
};
