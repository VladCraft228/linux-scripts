#include "windowmodel.h"
WindowModel::WindowModel(Runtime *r, QObject *p)
    : QAbstractListModel(p), runtime_(r), rows_(r->tasks) {
  connect(r, &Runtime::tasksUpdated, this, [this] {
    const auto next = runtime_->tasks;
    bool same = rows_.size() == next.size();
    if (same)
      for (int i = 0; i < next.size(); ++i)
        same &=
            rows_[i].toObject()["windowId"] == next[i].toObject()["windowId"];
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
  connect(r, &Runtime::stateUpdated, this, [this] {
    const auto s = runtime_->state;
    const auto update = [](bool &old, bool next, const auto &notify) {
      if (old != next) {
        old = next;
        notify();
      }
    };
    update(inside_, s["pointerInside"].toBool(),
           [this] { emit pointerInsideChanged(); });
    update(edge_, s["edgeHovered"].toBool(),
           [this] { emit edgeHoveredChanged(); });
    update(obstructed_, s["dockObstructed"].toBool(),
           [this] { emit dockObstructedChanged(); });
    update(desktop_, s["desktopVisible"].toBool(),
           [this] { emit desktopVisibleChanged(); });
  });
}
void WindowModel::attach(QQuickWindow *w) {
  window_ = w;
  runtime_->request({{"op", "attach"},
                     {"wid", qint64(w->winId())},
                     {"scale", w->devicePixelRatio()},
                     {"width", w->width()},
                     {"height", w->height()}});
}
int WindowModel::rowCount(const QModelIndex &p) const {
  return p.isValid() ? 0 : rows_.size();
}
QHash<int, QByteArray> WindowModel::roleNames() const {
  return {{WindowId, "windowId"},     {Title, "windowTitle"},
          {IconUrl, "iconUrl"},       {Active, "windowActive"},
          {ClassNames, "classNames"}, {DesktopId, "desktopId"}};
}
QVariant WindowModel::data(const QModelIndex &i, int role) const {
  if (!i.isValid() || i.row() < 0 || i.row() >= rows_.size())
    return {};
  return rows_[i.row()]
      .toObject()
      .value(QString::fromUtf8(roleNames().value(role)))
      .toVariant();
}
void WindowModel::activate(quint32 id) {
  runtime_->request({{"op", "activateWindow"}, {"wid", qint64(id)}});
}
void WindowModel::minimize(quint32 id) {
  runtime_->request({{"op", "minimizeWindow"}, {"wid", qint64(id)}});
}
void WindowModel::closeWindow(quint32 id) {
  runtime_->request({{"op", "closeWindow"}, {"wid", qint64(id)}});
}
void WindowModel::setDockHidden(bool hidden) {
  runtime_->request({{"op", "hidden"}, {"hidden", hidden}});
}
void WindowModel::setDockArea(qreal x, qreal y, qreal width, qreal height) {
  runtime_->request(
      {{"op", "area"},
       {"rect",
        QJsonObject{
            {"x", x}, {"y", y}, {"width", width}, {"height", height}}}});
}
void WindowModel::setInputRegion(const QVariantList &rects) {
  if (!window_)
    return;
  const auto region = QJsonArray::fromVariantList(rects);
  const auto scale = window_->devicePixelRatio();
  if (region == lastRegion_ && scale == lastScale_)
    return;
  lastRegion_ = region;
  lastScale_ = scale;
  runtime_->request({{"op", "region"},
                     {"rects", region},
                     {"scale", scale},
                     {"width", window_->width()},
                     {"height", window_->height()}});
}
