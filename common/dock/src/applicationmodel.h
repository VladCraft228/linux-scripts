#pragma once
#include "runtime.h"
#include <QAbstractListModel>
class ApplicationModel final : public QAbstractListModel {
  Q_OBJECT
  Q_PROPERTY(QVariantMap preferences READ preferences NOTIFY preferencesChanged)
  Q_PROPERTY(
      bool autoHide READ autoHide WRITE setAutoHide NOTIFY autoHideChanged)
public:
  enum Role {
    Key = Qt::UserRole + 1,
    Name,
    IconUrl,
    Active,
    RunningCount,
    Pinned,
    Launching
  };
  explicit ApplicationModel(Runtime *runtime, QObject *parent = nullptr);
  int rowCount(const QModelIndex &parent = {}) const override;
  QVariant data(const QModelIndex &index, int role) const override;
  QHash<int, QByteArray> roleNames() const override;
  QImage icon(const QString &key) const {
    return runtime_->applicationIcon(key);
  }
  QVariantMap preferences() const {
    return runtime_->config["preferences"].toObject().toVariantMap();
  }
  bool autoHide() const { return runtime_->config["autoHide"].toBool(); }
  void setAutoHide(bool enabled);
  Q_INVOKABLE QVariantList availableApplications() const;
  Q_INVOKABLE bool pinApplication(const QString &id);
  Q_INVOKABLE void resetPreferences();
  Q_INVOKABLE void setPreference(const QString &name, int value);
  Q_INVOKABLE void activate(const QString &key);
  Q_INVOKABLE bool acceptsDesktopFiles(const QVariantList &urls) const;
  Q_INVOKABLE bool pinDesktopFiles(const QVariantList &urls, int position = -1);
  Q_INVOKABLE bool reorderPinned(const QString &key, const QString &targetKey);
  Q_INVOKABLE QVariantList previewWindows(const QString &key) const;
  Q_INVOKABLE QVariantList menu(const QString &key) const;
  Q_INVOKABLE void perform(const QString &action, const QString &key,
                           quint32 windowId = 0);
signals:
  void preferencesChanged();
  void autoHideChanged();
  void launchError(const QString &message);

private:
  Runtime *runtime_;
  QJsonArray rows_;
  QVariantMap preferences_;
  bool autoHide_;
};
