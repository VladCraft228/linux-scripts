#pragma once
#include "runtime.h"
#include <QAbstractListModel>
#include <QQuickWindow>
class WindowModel final : public QAbstractListModel {
  Q_OBJECT
  Q_PROPERTY(bool pointerInside READ pointerInside NOTIFY pointerInsideChanged)
  Q_PROPERTY(bool edgeHovered READ edgeHovered NOTIFY edgeHoveredChanged)
  Q_PROPERTY(
      bool dockObstructed READ dockObstructed NOTIFY dockObstructedChanged)
  Q_PROPERTY(
      bool desktopVisible READ desktopVisible NOTIFY desktopVisibleChanged)
public:
  enum Role {
    WindowId = Qt::UserRole + 1,
    Title,
    IconUrl,
    Active,
    ClassNames,
    DesktopId
  };
  explicit WindowModel(Runtime *runtime, QObject *parent = nullptr);
  bool valid() const { return runtime_->valid(); }
  bool pointerInside() const { return inside_; }
  bool edgeHovered() const { return edge_; }
  bool dockObstructed() const { return obstructed_; }
  bool desktopVisible() const { return desktop_; }
  void attach(QQuickWindow *window);
  int rowCount(const QModelIndex &parent = {}) const override;
  QVariant data(const QModelIndex &index, int role) const override;
  QHash<int, QByteArray> roleNames() const override;
  QImage icon(quint32 id) const { return runtime_->taskIcon(id); }
  Q_INVOKABLE void activate(quint32 id);
  void minimize(quint32 id);
  void closeWindow(quint32 id);
  Q_INVOKABLE void setInputRegion(const QVariantList &rects);
  Q_INVOKABLE void setDockArea(qreal x, qreal y, qreal width, qreal height);
  Q_INVOKABLE void setDockHidden(bool hidden);
signals:
  void pointerInsideChanged();
  void edgeHoveredChanged();
  void dockObstructedChanged();
  void desktopVisibleChanged();

private:
  Runtime *runtime_;
  QQuickWindow *window_ = nullptr;
  QJsonArray rows_, lastRegion_;
  qreal lastScale_ = 0;
  bool inside_ = false, edge_ = false, obstructed_ = false, desktop_ = true;
};
