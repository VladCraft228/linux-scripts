#pragma once

#include <QAbstractListModel>
#include <QHash>
#include <QImage>
#include <QQuickWindow>
#include <QRegion>
#include <QSocketNotifier>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <xcb/xcb.h>

// Technical prototype: one model entry per window, not yet per application.
class WindowModel final : public QAbstractListModel {
    Q_OBJECT
public:
    enum Role { WindowId = Qt::UserRole + 1, Title, IconUrl, Active };
    explicit WindowModel(QObject *parent = nullptr);
    ~WindowModel() override;
    bool valid() const;
    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    QImage icon(xcb_window_t id) const;
    void attach(QQuickWindow *window);
    Q_INVOKABLE void activate(quint32 id);
    Q_INVOKABLE void setInputRegion(const QVariantList &rectangles);

private:
    struct Task {
        xcb_window_t id;
        QString title;
        QString iconKey;
        QImage image;
        bool active;
        quint64 revision;
    };
    xcb_atom_t atom(const QByteArray &name);
    QByteArray property(xcb_window_t window, const QByteArray &name,
                        xcb_atom_t type, uint32_t limit = 4096);
    QVector<quint32> cardinals(xcb_window_t window, const QByteArray &name,
                              xcb_atom_t type = XCB_ATOM_ATOM);
    void drain();
    void refresh();
    QImage loadIcon(xcb_window_t id, const QString &iconKey);
    xcb_connection_t *connection_ = nullptr;
    xcb_window_t root_ = XCB_WINDOW_NONE;
    QHash<QByteArray, xcb_atom_t> atoms_;
    QVector<Task> tasks_;
    QSet<xcb_window_t> dirtyIcons_;
    QSocketNotifier *notifier_ = nullptr;
    QTimer refreshTimer_;
    QQuickWindow *window_ = nullptr;
    quint64 revision_ = 0;
    QRegion inputRegion_;
    qreal inputScale_ = 0;
};
