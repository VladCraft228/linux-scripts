#include "applicationmodel.h"
#include <QUrl>
ApplicationModel::ApplicationModel(Runtime *r, QObject *p)
    : QAbstractListModel(p), runtime_(r), rows_(r->groups),
      preferences_(preferences()), autoHide_(autoHide()) {
  connect(r, &Runtime::error, this, &ApplicationModel::launchError);
  connect(r, &Runtime::configUpdated, this, [this] {
    if (preferences_ != preferences()) {
      preferences_ = preferences();
      emit preferencesChanged();
    }
    if (autoHide_ != autoHide()) {
      autoHide_ = autoHide();
      emit autoHideChanged();
    }
  });
  connect(r, &Runtime::groupsUpdated, this, [this] {
    const auto next = runtime_->groups;
    bool same = rows_.size() == next.size();
    if (same)
      for (int i = 0; i < rows_.size(); ++i)
        same &= rows_[i].toObject()["appKey"] == next[i].toObject()["appKey"];
    if (!same) {
      beginResetModel();
      rows_ = next;
      endResetModel();
    } else
      for (int i = 0; i < next.size(); ++i)
        if (rows_[i] != next[i]) {
          rows_[i] = next[i];
          emit dataChanged(index(i), index(i));
        }
  });
}
int ApplicationModel::rowCount(const QModelIndex &p) const {
  return p.isValid() ? 0 : rows_.size();
}
QHash<int, QByteArray> ApplicationModel::roleNames() const {
  return {{Key, "appKey"},
          {Name, "appName"},
          {IconUrl, "iconUrl"},
          {Active, "appActive"},
          {RunningCount, "runningCount"},
          {Pinned, "pinned"},
          {Launching, "launching"}};
}
QVariant ApplicationModel::data(const QModelIndex &i, int role) const {
  if (!i.isValid() || i.row() < 0 || i.row() >= rows_.size())
    return {};
  return rows_[i.row()]
      .toObject()
      .value(QString::fromUtf8(roleNames().value(role)))
      .toVariant();
}
void ApplicationModel::setPreference(const QString &name, int value) {
  runtime_->request({{"op", "preference"}, {"name", name}, {"value", value}});
}
void ApplicationModel::setAutoHide(bool enabled) {
  runtime_->request({{"op", "autoHide"}, {"enabled", enabled}});
}
void ApplicationModel::activate(const QString &key) {
  runtime_->request({{"op", "activate"}, {"key", key}});
}
static QJsonArray urlsArray(const QVariantList &urls) {
  QJsonArray a;
  for (const auto &v : urls)
    a.append(v.toUrl().toString());
  return a;
}
bool ApplicationModel::acceptsDesktopFiles(const QVariantList &urls) const {
  return runtime_->request({{"op", "acceptsFiles"}, {"urls", urlsArray(urls)}})
      .toBool();
}
bool ApplicationModel::pinDesktopFiles(const QVariantList &urls, int pos) {
  return runtime_
      ->request(
          {{"op", "pinFiles"}, {"urls", urlsArray(urls)}, {"position", pos}})
      .toBool();
}
bool ApplicationModel::reorderPinned(const QString &key,
                                     const QString &target) {
  return runtime_
      ->request({{"op", "reorder"}, {"key", key}, {"target", target}})
      .toBool();
}
QVariantList ApplicationModel::previewWindows(const QString &key) const {
  return runtime_->request({{"op", "previews"}, {"key", key}})
      .toArray()
      .toVariantList();
}
QVariantList ApplicationModel::menu(const QString &key) const {
  return runtime_->request({{"op", "menu"}, {"key", key}})
      .toArray()
      .toVariantList();
}
void ApplicationModel::perform(const QString &action, const QString &key,
                               quint32 wid) {
  runtime_->request({{"op", "perform"},
                     {"action", action},
                     {"key", key},
                     {"wid", qint64(wid)}});
}

void ApplicationModel::resetPreferences() {
  runtime_->request({{"op", "resetPreferences"}});
}

QVariantList ApplicationModel::availableApplications() const {
  return runtime_->request({{"op", "availableApplications"}})
      .toArray()
      .toVariantList();
}
bool ApplicationModel::pinApplication(const QString &id) {
  return runtime_->request({{"op", "pinApplication"}, {"id", id}}).toBool();
}
