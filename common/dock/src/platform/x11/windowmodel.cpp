#include "windowmodel.h"

#include <QIcon>
#include <QPainter>
#include <QRegion>
#include <QSet>
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <xcb/shape.h>

WindowModel::WindowModel(QObject *parent) : QAbstractListModel(parent) {
    int screenNumber = 0;
    connection_ = xcb_connect(nullptr, &screenNumber);
    if (!valid()) return;
    const auto *shape = xcb_get_extension_data(connection_, &xcb_shape_id);
    if (!shape || !shape->present) {
        qWarning("The X11 SHAPE extension is required.");
        xcb_disconnect(connection_);
        connection_ = nullptr;
        return;
    }
    auto screens = xcb_setup_roots_iterator(xcb_get_setup(connection_));
    while (screenNumber-- > 0) xcb_screen_next(&screens);
    if (!screens.data) return;
    root_ = screens.data->root;
    const uint32_t mask = XCB_EVENT_MASK_PROPERTY_CHANGE;
    xcb_change_window_attributes(connection_, root_, XCB_CW_EVENT_MASK, &mask);
    refreshTimer_.setSingleShot(true);
    refreshTimer_.setInterval(25);
    connect(&refreshTimer_, &QTimer::timeout, this, &WindowModel::refresh);
    notifier_ = new QSocketNotifier(xcb_get_file_descriptor(connection_),
                                     QSocketNotifier::Read, this);
    connect(notifier_, &QSocketNotifier::activated, this, [this] { drain(); });
    xcb_flush(connection_);
}

WindowModel::~WindowModel() {
    if (notifier_) notifier_->setEnabled(false);
    if (connection_) xcb_disconnect(connection_);
}

bool WindowModel::valid() const {
    return connection_ && !xcb_connection_has_error(connection_);
}

xcb_atom_t WindowModel::atom(const QByteArray &name) {
    if (atoms_.contains(name)) return atoms_.value(name);
    auto *reply = xcb_intern_atom_reply(connection_,
        xcb_intern_atom(connection_, false, uint16_t(name.size()), name.constData()), nullptr);
    const auto result = reply ? reply->atom : xcb_atom_t(XCB_ATOM_NONE);
    free(reply);
    atoms_.insert(name, result);
    return result;
}

QByteArray WindowModel::property(xcb_window_t window, const QByteArray &name,
                                xcb_atom_t type, uint32_t limit) {
    auto *reply = xcb_get_property_reply(connection_,
        xcb_get_property(connection_, false, window, atom(name), type, 0, limit), nullptr);
    QByteArray value;
    if (reply && reply->type == type) {
        value = QByteArray(static_cast<const char *>(xcb_get_property_value(reply)),
                           xcb_get_property_value_length(reply));
    }
    free(reply);
    return value;
}

QVector<quint32> WindowModel::cardinals(xcb_window_t window, const QByteArray &name,
                                      xcb_atom_t type) {
    const auto bytes = property(window, name, type);
    QVector<quint32> result(bytes.size() / 4);
    if (!result.isEmpty()) std::memcpy(result.data(), bytes.constData(), size_t(result.size()) * 4);
    return result;
}

int WindowModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : int(tasks_.size());
}

QVariant WindowModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= tasks_.size()) return {};
    const auto &task = tasks_[index.row()];
    switch (role) {
    case WindowId: return task.id;
    case Title: return task.title;
    case IconUrl: return QStringLiteral("image://task/%1/%2").arg(task.id).arg(task.revision);
    case Active: return task.active;
    default: return {};
    }
}

QHash<int, QByteArray> WindowModel::roleNames() const {
    return {{WindowId, "windowId"}, {Title, "windowTitle"},
            {IconUrl, "iconUrl"}, {Active, "windowActive"}};
}

QImage WindowModel::icon(xcb_window_t id) const {
    for (const auto &task : tasks_) if (task.id == id) return task.image;
    return {};
}

QImage WindowModel::loadIcon(xcb_window_t id, const QString &key) {
    // First try the selected freedesktop icon theme. Matching is intentionally
    // basic until ApplicationCatalog adds desktop-file identity resolution.
    auto themed = QIcon::fromTheme(key);
    if (!themed.isNull()) return themed.pixmap(128, 128).toImage();
    const auto bytes = property(id, "_NET_WM_ICON", XCB_ATOM_CARDINAL, 262144);
    QVector<quint32> pixels(bytes.size() / 4);
    if (!pixels.isEmpty()) std::memcpy(pixels.data(), bytes.constData(), size_t(pixels.size()) * 4);
    qsizetype best = -1;
    quint32 bestSize = 0;
    for (qsizetype i = 0; i + 1 < pixels.size();) {
        const quint32 w = pixels[i], h = pixels[i + 1];
        const quint64 count = quint64(w) * h;
        if (!w || !h || w > 1024 || h > 1024 || count > quint64(pixels.size() - i - 2)) break;
        if (best < 0 || (w >= 96 && (bestSize < 96 || w < bestSize))) {
            best = i; bestSize = w;
        }
        i += qsizetype(count) + 2;
    }
    if (best >= 0) {
        QImage image(int(pixels[best]), int(pixels[best + 1]), QImage::Format_ARGB32);
        const auto *source = pixels.constData() + best + 2;
        for (int y = 0; y < image.height(); ++y)
            std::memcpy(image.scanLine(y), source + y * image.width(), size_t(image.width()) * 4);
        return image.scaled(128, 128, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }
    QImage fallback(128, 128, QImage::Format_ARGB32_Premultiplied);
    fallback.fill(Qt::transparent);
    QPainter painter(&fallback);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setBrush(QColor("#657fbc"));
    painter.setPen(Qt::NoPen);
    painter.drawRoundedRect(QRectF(8, 8, 112, 112), 25, 25);
    painter.setPen(Qt::white);
    QFont font = painter.font(); font.setPixelSize(54); painter.setFont(font);
    painter.drawText(fallback.rect(), Qt::AlignCenter, key.left(1).toUpper());
    return fallback;
}

void WindowModel::attach(QQuickWindow *window) {
    window_ = window;
    const xcb_atom_t type = atom("_NET_WM_WINDOW_TYPE_DOCK");
    xcb_change_property(connection_, XCB_PROP_MODE_REPLACE, xcb_window_t(window->winId()),
                        atom("_NET_WM_WINDOW_TYPE"), XCB_ATOM_ATOM, 32, 1, &type);
    // No struts during the technical prototype: leave the existing panel alone.
    xcb_flush(connection_);
    refresh();
}

void WindowModel::drain() {
    if (!valid()) {
        notifier_->setEnabled(false);
        qWarning("X11 connection lost. Close and restart the dock.");
        return;
    }
    bool changed = false;
    while (auto *event = xcb_poll_for_event(connection_)) {
        if ((event->response_type & 0x7f) == XCB_PROPERTY_NOTIFY) {
            auto *propertyEvent = reinterpret_cast<xcb_property_notify_event_t *>(event);
            const auto name = propertyEvent->atom;
            if (name == atom("_NET_WM_ICON") || name == atom("WM_CLASS"))
                dirtyIcons_.insert(propertyEvent->window);
            changed |= name == atom("_NET_CLIENT_LIST") || name == atom("_NET_ACTIVE_WINDOW")
                || name == atom("_NET_WM_NAME") || name == atom("WM_NAME")
                || name == atom("_NET_WM_STATE") || name == atom("_NET_WM_WINDOW_TYPE")
                || name == atom("_NET_WM_ICON") || name == atom("WM_CLASS");
        }
        free(event);
    }
    if (changed && !refreshTimer_.isActive()) refreshTimer_.start();
}

void WindowModel::refresh() {
    if (!valid()) return;
    const auto clients = cardinals(root_, "_NET_CLIENT_LIST", XCB_ATOM_WINDOW);
    const auto active = cardinals(root_, "_NET_ACTIVE_WINDOW", XCB_ATOM_WINDOW);
    const xcb_window_t activeId = active.isEmpty() ? xcb_window_t(XCB_WINDOW_NONE) : active.first();
    QVector<Task> updated;
    for (auto id : clients) {
        if (window_ && id == window_->winId()) continue;
        const uint32_t mask = XCB_EVENT_MASK_PROPERTY_CHANGE;
        xcb_change_window_attributes(connection_, id, XCB_CW_EVENT_MASK, &mask);
        const auto types = cardinals(id, "_NET_WM_WINDOW_TYPE");
        const auto states = cardinals(id, "_NET_WM_STATE");
        if (types.contains(atom("_NET_WM_WINDOW_TYPE_DOCK"))
            || types.contains(atom("_NET_WM_WINDOW_TYPE_DESKTOP"))
            || states.contains(atom("_NET_WM_STATE_SKIP_TASKBAR"))) continue;
        auto title = QString::fromUtf8(property(id, "_NET_WM_NAME", atom("UTF8_STRING")));
        if (title.isEmpty()) title = QString::fromLocal8Bit(property(id, "WM_NAME", XCB_ATOM_STRING));
        const auto classes = property(id, "WM_CLASS", XCB_ATOM_STRING).split('\0');
        QString key = QString::fromLocal8Bit(classes.size() > 1 ? classes[1] : classes.value(0)).toLower();
        if (key.isEmpty()) key = QStringLiteral("application-x-executable");
        const auto old = std::find_if(tasks_.cbegin(), tasks_.cend(), [id](const Task &task) { return task.id == id; });
        const bool reload = old == tasks_.cend() || old->iconKey != key || dirtyIcons_.contains(id);
        updated.append({id, title.isEmpty() ? key : title, key,
                        reload ? loadIcon(id, key) : old->image, id == activeId,
                        reload ? ++revision_ : old->revision});
    }
    dirtyIcons_.clear();
    const bool sameIds = updated.size() == tasks_.size()
        && std::equal(updated.cbegin(), updated.cend(), tasks_.cbegin(),
                      [](const Task &a, const Task &b) { return a.id == b.id; });
    if (!sameIds) {
        beginResetModel(); tasks_ = updated; endResetModel();
    } else {
        for (int i = 0; i < updated.size(); ++i) {
            const auto &a = tasks_[i]; const auto &b = updated[i];
            const bool changed = a.title != b.title || a.active != b.active || a.revision != b.revision;
            tasks_[i] = b;
            if (changed) emit dataChanged(index(i), index(i), {Title, IconUrl, Active});
        }
    }
    xcb_flush(connection_);
    // Synchronous property replies can buffer events inside XCB without leaving
    // bytes on the fd. Drain those too rather than waiting for another wakeup.
    drain();
}

void WindowModel::activate(quint32 id) {
    if (!valid() || std::none_of(tasks_.cbegin(), tasks_.cend(),
                                 [id](const Task &task) { return task.id == id; })) return;
    xcb_client_message_event_t event{};
    event.response_type = XCB_CLIENT_MESSAGE;
    event.format = 32;
    event.window = id;
    event.type = atom("_NET_ACTIVE_WINDOW");
    event.data.data32[0] = 2; // pager/taskbar request
    event.data.data32[1] = XCB_CURRENT_TIME;
    xcb_send_event(connection_, false, root_, XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT
                   | XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY, reinterpret_cast<const char *>(&event));
    xcb_flush(connection_);
}

void WindowModel::setInputRegion(const QVariantList &rectangles) {
    if (!window_) return;
    QRegion region;
    for (const auto &entry : rectangles) {
        const auto rect = entry.toMap();
        region += QRectF(rect.value("x").toDouble(), rect.value("y").toDouble(),
                         rect.value("width").toDouble(), rect.value("height").toDouble()).toAlignedRect();
    }
    const auto scale = window_->devicePixelRatio();
    if (region == inputRegion_ && scale == inputScale_) return;
    inputRegion_ = region;
    inputScale_ = scale;
    QVector<xcb_rectangle_t> native;
    for (const auto &rect : region) {
        const auto scaled = QRectF(rect.x() * scale, rect.y() * scale,
                                    rect.width() * scale, rect.height() * scale).toAlignedRect();
        native.append({int16_t(scaled.x()), int16_t(scaled.y()),
                       uint16_t(scaled.width()), uint16_t(scaled.height())});
    }
    // ShapeInput only: keep tooltip rendering intact outside the input region.
    xcb_shape_rectangles(connection_, XCB_SHAPE_SO_SET, XCB_SHAPE_SK_INPUT,
                         XCB_CLIP_ORDERING_UNSORTED, xcb_window_t(window_->winId()),
                         0, 0, uint32_t(native.size()), native.constData());
    xcb_flush(connection_);
}
