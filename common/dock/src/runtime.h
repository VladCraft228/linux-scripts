#pragma once
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QSettings>
#include <QSocketNotifier>
#include <QTimer>
#include <memory>
namespace dock {
class RustEngine;
}
// Qt-only adapter: event-loop scheduling, QSettings compatibility and image
// rendering. All application and X11 policy lives in rust/src.
class Runtime final : public QObject {
  Q_OBJECT
public:
  explicit Runtime(QObject *parent = nullptr);
  ~Runtime() override;
  QJsonValue request(QJsonObject request);
  bool valid() const { return valid_; }
  QJsonArray groups, tasks;
  QJsonObject config, state;
  QImage taskIcon(quint32 id) const { return images_.value(id); }
  QImage applicationIcon(const QString &key) const;
  QImage catalogIcon(const QString &id);
signals:
  void groupsUpdated();
  void tasksUpdated();
  void stateUpdated();
  void configUpdated();
  void error(const QString &message);

private:
  void apply(const QJsonObject &response);
  std::unique_ptr<dock::RustEngine> engine_;
  QSettings settings_;
  QTimer refresh_, fallback_, launch_;
  QSocketNotifier *notifier_ = nullptr;
  QHash<quint32, QImage> images_;
  bool valid_ = false;
};
