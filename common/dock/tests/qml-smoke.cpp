#include <QAbstractListModel>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QQuickWindow>
#include <QQuickItem>
#include <cmath>
#include <QTimer>
#include <QMimeData>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QUrl>
#include <QtTest/QTest>

// Exercise the actual QML with live model mutations, without a display server.
// This checks the UI/model contract; it does not replace X11 integration testing.
class Fixture final : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(QVariantMap preferences READ preferences NOTIFY preferencesChanged)
    Q_PROPERTY(bool dockObstructed READ dockObstructed NOTIFY dockObstructedChanged)
    Q_PROPERTY(bool desktopVisible READ desktopVisible NOTIFY desktopVisibleChanged)
    Q_PROPERTY(bool autoHide READ autoHide WRITE setAutoHide NOTIFY autoHideChanged)
    Q_PROPERTY(bool pointerInside READ pointerInside NOTIFY pointerInsideChanged)
    Q_PROPERTY(bool edgeHovered READ edgeHovered NOTIFY edgeHoveredChanged)
public:
    QVariantMap preferences_{{"indicatorStyle",0},{"indicatorActiveColor",7910399},{"indicatorRunningColor",9280174},{"iconSpacing",10},{"iconSize",48},{"magnification",55},{"opacity",85},{"bottomOffset",96},{"hideDelay",700},{"animationDuration",120},{"backgroundColor",2764860},{"borderOpacity",38},{"borderWidth",1},{"borderColor",16777215},{"borderEnabled",1},{"cornerRadius",22}};
    QVariantMap preferences() const { return preferences_; }
    Q_INVOKABLE void setPreference(const QString &key,int value) { preferences_[key] = value; emit preferencesChanged(); }
    bool dockObstructed() const { return obstructed_; }
    bool obstructed_ = true;
    bool desktopVisible() const { return desktopVisible_; }
    bool desktopVisible_ = false;
    void setDesktopVisible(bool value) { desktopVisible_ = value; obstructed_ = !value; emit dockObstructedChanged(); emit desktopVisibleChanged(); }
    bool pointerInside() const { return inside_; }
    bool edgeHovered() const { return edge_; }
    bool inside_ = true;
    bool edge_ = false;
    void leaveDock() { inside_ = false; emit pointerInsideChanged(); }
    void reachEdge() { inside_ = true; edge_ = true; emit pointerInsideChanged(); emit edgeHoveredChanged(); }
    bool autoHide() const { return autoHide_; }
    bool autoHide_ = false;
    void setAutoHide(bool value) { autoHide_ = value; emit autoHideChanged(); }
    int count = 3;
    bool launching_ = false;
    void setLaunching(bool value) {
        launching_ = value;
        if (count > 1) emit dataChanged(index(1), index(1), {Qt::UserRole + 7});
    }
    int chooserPins = 0;
    bool chooserPinned = false;
    Q_INVOKABLE QVariantList availableApplications() const {
        return {QVariantMap{{"id","editor.desktop"},{"name","Fake Editor"},{"pinned",chooserPinned}},QVariantMap{{"id","calculator.desktop"},{"name","Calculator"},{"pinned",false}}};
    }
    Q_INVOKABLE bool pinApplication(const QString &id) { if(id!="editor.desktop")return false; ++chooserPins;chooserPinned=true;return true; }
    int resetCalls = 0;
    Q_INVOKABLE void resetPreferences() { ++resetCalls; }
    int maskCalls = 0;
    int reorderCalls = 0;
    int closeCalls = 0;
    int dropCalls = 0;
    quint32 closedWindow = 0;
    QString movedKey;
    QString targetKey;
    int rowCount(const QModelIndex &parent = {}) const override { return parent.isValid() ? 0 : count; }
    QVariant data(const QModelIndex &index, int role) const override {
        if (role == Qt::UserRole + 1) return QString::number(index.row() + 1);
        if (role == Qt::UserRole + 2) return QString("Test window %1").arg(index.row());
        if (role == Qt::UserRole + 3) return QString("image://task/%1/0").arg(index.row());
        if (role == Qt::UserRole + 4) return index.row() == 0;
        if (role == Qt::UserRole + 5) return 2;
        if (role == Qt::UserRole + 6) return true;
        if (role == Qt::UserRole + 7) return index.row() == 1 && launching_;
        return {};
    }
    QHash<int, QByteArray> roleNames() const override {
        return {{Qt::UserRole + 1, "appKey"}, {Qt::UserRole + 2, "appName"},
                {Qt::UserRole + 3, "iconUrl"}, {Qt::UserRole + 4, "appActive"},
                {Qt::UserRole + 5, "runningCount"}, {Qt::UserRole + 6, "pinned"},
                {Qt::UserRole + 7,"launching"}};
    }
    Q_INVOKABLE void setInputRegion(const QVariantList &rects) {
        if (!rects.isEmpty()) ++maskCalls;
    }
    Q_INVOKABLE void setDockHidden(bool) {}
    Q_INVOKABLE void setDockArea(qreal,qreal,qreal,qreal) {}
    Q_INVOKABLE void activate(const QString &) {}
    Q_INVOKABLE QVariantList menu(const QString &) const {
        return {QVariantMap{{"label", "Pin application"}, {"action", "pin"}, {"enabled", true}, {"wid", 0}}};
    }
    Q_INVOKABLE void perform(const QString &action, const QString &, quint32 id) {
        if (action == "close") { ++closeCalls; closedWindow = id; }
    }
    Q_INVOKABLE bool acceptsDesktopFiles(const QVariantList &urls) const { return urls.size() == 1; }
    Q_INVOKABLE bool pinDesktopFiles(const QVariantList &urls, int position) {
        if (urls.size() != 1 || position < 0) return false;
        ++dropCalls;
        return true;
    }
    Q_INVOKABLE bool reorderPinned(const QString &from, const QString &to) {
        ++reorderCalls; movedKey = from; targetKey = to; return true;
    }
    Q_INVOKABLE QVariantList previewWindows(const QString &) const {
        return {QVariantMap{{"wid",1},{"title","Test window"},{"iconUrl","image://task/0/0"},{"active",true}}};
    }
Q_SIGNALS:
    void preferencesChanged();
    void dockObstructedChanged();
    void desktopVisibleChanged();
    void pointerInsideChanged();
    void edgeHoveredChanged();
    void autoHideChanged();
    void launchError(const QString &text);
public:
    void empty() { beginResetModel(); count = 0; endResetModel(); }
    void restore() { beginResetModel(); count = 3; endResetModel(); }
};

class Icons final : public QQuickImageProvider {
public:
    Icons() : QQuickImageProvider(Image) {}
    QImage requestImage(const QString &, QSize *size, const QSize &) override {
        QImage image(128, 128, QImage::Format_ARGB32_Premultiplied);
        image.fill(QColor("#74a5f2"));
        if (size) *size = image.size();
        return image;
    }
};

static QQuickItem *visualChild(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name) return parent;
    for (auto *child : parent->childItems())
        if (auto *found = visualChild(child,name)) return found;
    return nullptr;
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    Fixture fixture;
    QQmlApplicationEngine engine;
    int warnings = 0;
    QObject::connect(&engine, &QQmlEngine::warnings, &app,
                     [&warnings](const QList<QQmlError> &errors) { warnings += int(errors.size()); });
    engine.rootContext()->setContextProperty("windowModel", &fixture);
    engine.rootContext()->setContextProperty("applicationModel", &fixture);
    engine.addImageProvider("task", new Icons);
    engine.addImageProvider("catalog", new Icons);
    engine.load(QUrl("qrc:/qml/Main.qml"));
    if (engine.rootObjects().isEmpty()) return 1;
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window) return 1;
    window->show();
    if (!qEnvironmentVariableIsEmpty("DOCK_TEST_SNAPSHOT")) {
        QTimer::singleShot(285, Qt::PreciseTimer, window, [window] {
            window->grabWindow().save(qEnvironmentVariable("DOCK_TEST_SNAPSHOT"));
        });
    }
    QTimer::singleShot(310, Qt::PreciseTimer, window, [window] {
        QMetaObject::invokeMethod(window,"openSettings");
    });
    bool settingsOk = false;
    QTimer::singleShot(350, Qt::PreciseTimer, window, [window,&settingsOk,&fixture] {
        auto *settings = window->findChild<QQuickWindow *>("dockSettings");
        settingsOk = settings && settings->isVisible();
        if (settings && !qEnvironmentVariableIsEmpty("DOCK_SETTINGS_SNAPSHOT"))
            settings->grabWindow().save(qEnvironmentVariable("DOCK_SETTINGS_SNAPSHOT"));
        if (settings) {
            window->setProperty("settingsPage",1); QTest::qWait(20);
            auto *sizeSlider=visualChild(settings->contentItem(),"setting-iconSize");
            settingsOk &= sizeSlider!=nullptr;
            if(sizeSlider)QTest::mouseClick(settings,Qt::LeftButton,Qt::NoModifier,sizeSlider->mapToScene(QPointF(330,31)).toPoint());
            settingsOk &= fixture.preferences().value("iconSize").toInt() > 48;
            fixture.setPreference("iconSize",48);
            window->setProperty("settingsPage",0); QTest::qWait(20);
            auto *panel = visualChild(window->contentItem(),"dockPanel");
            const double panelHeight=panel?panel->height():0;
            const double panelBottom=panel?panel->y()+panel->height():0;
            fixture.setPreference("iconSpacing",24);
            settingsOk &= window->property("gap").toInt()==24;
            settingsOk &= panel && std::abs(panel->width()-std::max(180.0,window->property("baseWidth").toDouble()+32))<0.5;
            settingsOk &= panel && panel->height()==panelHeight && panel->y()+panel->height()==panelBottom;
            fixture.setPreference("iconSpacing",10);
            fixture.setPreference("opacity",0);
            fixture.setPreference("borderEnabled",1);
            fixture.setPreference("cornerRadius",0);
            settingsOk &= panel && panel->property("color").value<QColor>().alpha()==0 && panel->property("radius").toDouble()==0;
            auto *border = panel ? panel->property("border").value<QObject *>() : nullptr;
            fixture.setPreference("borderEnabled",0);
            settingsOk &= border && border->property("width").toDouble()==0;
            fixture.setPreference("borderEnabled",1);
            fixture.setPreference("borderOpacity",0);
            settingsOk &= border && border->property("color").value<QColor>().alpha()==0;
            fixture.setPreference("borderOpacity",100);
            settingsOk &= border && border->property("color").value<QColor>().alpha()==255;
            fixture.setPreference("borderOpacity",38);
            QVariant accepted;
            QMetaObject::invokeMethod(window,"setPickerHex",Q_RETURN_ARG(QVariant,accepted),Q_ARG(QVariant,QVariant("#ff667a")));
            settingsOk &= accepted.toBool() && fixture.preferences().value("borderColor").toInt()==0xff667a;
            const auto borderColor = border ? border->property("color").value<QColor>() : QColor();
            settingsOk &= borderColor.red()==255 && borderColor.green()==102 && borderColor.blue()==122;
            QMetaObject::invokeMethod(window,"setPickerHsv",Q_ARG(QVariant,QVariant(0.0)),Q_ARG(QVariant,QVariant(1.0)),Q_ARG(QVariant,QVariant(1.0)));
            settingsOk &= fixture.preferences().value("borderColor").toInt()==0xff0000;
            auto *wheel = visualChild(settings->contentItem(),"borderColorWheel");
            auto *viewport = visualChild(settings->contentItem(),"settingsViewport");
            if(wheel && viewport) {
                const double top=wheel->mapToItem(viewport,QPointF(0,0)).y()+viewport->property("contentY").toDouble();
                viewport->setProperty("contentY",qBound(0.0,top-30,viewport->property("contentHeight").toDouble()-viewport->height()));
                QTest::mouseClick(settings,Qt::LeftButton,Qt::NoModifier,wheel->mapToScene(QPointF(158,88)).toPoint());
                const auto color=fixture.preferences().value("borderColor").toInt();
                settingsOk &= ((color>>16)&255)>200 && ((color>>8)&255)<65 && (color&255)<65;
            } else settingsOk=false;
            fixture.setPreference("borderWidth",5);
            settingsOk &= border && border->property("width").toDouble()==5;
            fixture.setPreference("borderWidth",1);
            fixture.setPreference("borderColor",16777215);
            const auto savedBorder=fixture.preferences().value("borderColor");
            window->setProperty("colorTarget","backgroundColor");
            QMetaObject::invokeMethod(window,"setPickerHex",Q_RETURN_ARG(QVariant,accepted),Q_ARG(QVariant,QVariant("#123456")));
            settingsOk &= accepted.toBool() && fixture.preferences().value("backgroundColor").toInt()==0x123456;
            settingsOk &= fixture.preferences().value("borderColor")==savedBorder;
            fixture.setPreference("opacity",100);
            const auto background=panel->property("color").value<QColor>();
            settingsOk &= background.red()==18 && background.green()==52 && background.blue()==86 && background.alpha()==255;
            window->setProperty("colorTarget","borderColor");
            fixture.setPreference("backgroundColor",0x2a303c);
            fixture.setPreference("opacity",85);
            fixture.setPreference("cornerRadius",22);
            qInfo("Palette checks: ok=%d",settingsOk);
            window->setProperty("settingsPage",3); QTest::qWait(20);
            auto *indicator=visualChild(window->contentItem(),"activeIndicator-1");
            fixture.setPreference("indicatorStyle",1);
            fixture.setPreference("indicatorActiveColor",0x12ff34);
            settingsOk &= indicator && indicator->width()==8 && indicator->height()==8 && indicator->property("color").value<QColor>().green()==255;
            fixture.setPreference("indicatorStyle",3);
            settingsOk &= indicator && indicator->property("radius").toDouble()==0;
            fixture.setPreference("indicatorStyle",4);
            settingsOk &= indicator && !indicator->isVisible();
            fixture.setPreference("indicatorStyle",0);fixture.setPreference("indicatorActiveColor",0x78b3ff);
            window->setProperty("settingsPage",0); QTest::qWait(20);
            qInfo("Indicator checks: ok=%d",settingsOk);
            auto *reset=visualChild(settings->contentItem(),"settingsResetButton");
            auto *cancel=visualChild(settings->contentItem(),"settingsResetCancel");
            const auto click=[settings](QQuickItem *item){if(item)QTest::mouseClick(settings,Qt::LeftButton,Qt::NoModifier,item->mapToScene(QPointF(item->width()/2,item->height()/2)).toPoint());};
            click(reset);QTest::qWait(20);settingsOk &= settings->property("resetPending").toBool() && fixture.resetCalls==0;
            click(cancel);QTest::qWait(20);settingsOk &= !settings->property("resetPending").toBool() && fixture.resetCalls==0;
            click(reset);click(reset);QTest::qWait(20);settingsOk &= !settings->property("resetPending").toBool() && fixture.resetCalls==1;
            qInfo("Reset checks: ok=%d calls=%d",settingsOk,fixture.resetCalls);
            settings->hide();
            QMetaObject::invokeMethod(window,"openAbout");
            auto *about=window->findChild<QQuickWindow *>("dockAbout");
            settingsOk &= about && about->isVisible();
            if(about) {
                if(!qEnvironmentVariableIsEmpty("DOCK_ABOUT_SNAPSHOT")) {
                    QTest::qWait(30);
                    about->grabWindow().save(qEnvironmentVariable("DOCK_ABOUT_SNAPSHOT"));
                }
                about->hide();
            }
        }
    });
    bool chooserOk = false;
    QTimer::singleShot(600, Qt::PreciseTimer, window, [window,&fixture,&chooserOk] {
        QMetaObject::invokeMethod(window,"openApplicationChooser");
        auto *chooser=window->findChild<QQuickWindow *>("applicationChooser");
        if(!chooser)return;
        auto *search=visualChild(chooser->contentItem(),"applicationSearch");
        auto *list=visualChild(chooser->contentItem(),"availableApplicationList");
        chooserOk=chooser->isVisible()&&search&&list;
        if(search&&list) {
            search->setProperty("text","Fake Editor");QTest::qWait(30);
            chooserOk &= list->property("count").toInt()==1;
            if(!qEnvironmentVariableIsEmpty("DOCK_CHOOSER_SNAPSHOT"))chooser->grabWindow().save(qEnvironmentVariable("DOCK_CHOOSER_SNAPSHOT"));
            QTest::mouseClick(chooser,Qt::LeftButton,Qt::NoModifier,list->mapToScene(QPointF(120,25)).toPoint());
            chooserOk &= fixture.chooserPins==1&&fixture.chooserPinned;
        }
        chooser->hide();
    });
    bool uniformIdleOk = false;
    QTimer::singleShot(10, Qt::PreciseTimer, window, [window, &uniformIdleOk] {
        auto *active = visualChild(window->contentItem(),"dockIcon-1");
        const auto base = window->property("baseSize").toDouble();
        uniformIdleOk = active && std::abs(active->width() - base) < 0.5;
    });
    QTimer::singleShot(20, Qt::PreciseTimer, window, [window] {
        QTest::mousePress(window, Qt::LeftButton, Qt::NoModifier, QPoint(442,510));
    });
    QTimer::singleShot(60, Qt::PreciseTimer, window, [window] {
        QTest::mouseMove(window, QPoint(558,510));
    });
    QTimer::singleShot(100, Qt::PreciseTimer, window, [window] {
        QTest::mouseRelease(window, Qt::LeftButton, Qt::NoModifier, QPoint(558,510));
    });
    QTimer::singleShot(120, Qt::PreciseTimer, window, [window] {
        QMetaObject::invokeMethod(window, "openMenu", Q_ARG(QVariant, QVariant("1")));
    });
    QTimer::singleShot(140, Qt::PreciseTimer, window, [window] { window->setProperty("menuOpen", false); });
    QTimer::singleShot(160, Qt::PreciseTimer, window, [window] {
        QMetaObject::invokeMethod(window, "showPreview", Q_ARG(QVariant, QVariant("1")));
    });
    bool geometryOk = false;
    QTimer::singleShot(280, Qt::PreciseTimer, window, [window, &geometryOk] {
        auto *panel = visualChild(window->contentItem(),"dockPanel");
        auto *first = visualChild(window->contentItem(),"dockIcon-1");
        auto *last = visualChild(window->contentItem(),"dockIcon-3");
        auto *neighbor = visualChild(window->contentItem(),"dockIcon-2");
        auto *mouse = visualChild(window->contentItem(),"dockMouse-3");
        auto *preview = visualChild(window->contentItem(),"windowPreview");
        if (!panel || !first || !last || !preview || !neighbor || !mouse) { qWarning("Missing layout test item"); return; }
        const bool contained = std::abs(panel->width() - std::max(180.0, window->property("baseWidth").toDouble() + 32)) < 0.5
            && std::abs(panel->height() - window->property("baseSize").toDouble() - 36) < 0.5
            && std::abs(panel->y() + panel->height() - window->property("baseline").toDouble() - 26) < 0.5;
        const bool enlarged = first->width() > window->property("baseSize").toDouble() * 1.4;
        const bool anchored = std::abs(preview->x() + preview->width()/2 - first->x() - first->width()/2) < 2
            && preview->y() + preview->height() <= first->y() - 10;
        const bool distinct = neighbor->width() < window->property("baseSize").toDouble() * 1.15
            && last->width() < window->property("baseSize").toDouble() * 1.15;
        const bool handCursor = mouse->property("cursorShape").toInt() == int(Qt::PointingHandCursor);
        geometryOk = contained && enlarged && anchored && distinct && handCursor;
        qInfo("Hover checks: distinct=%d handCursor=%d", distinct, handCursor);
        qInfo("Layout checks: contained=%d enlarged=%d anchored=%d", contained, enlarged, anchored);
    });
    QTimer::singleShot(290, Qt::PreciseTimer, window, [window] {
        auto *close = visualChild(window->contentItem(),"closeWindow-1");
        if (close) QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier,
                                    close->mapToScene(QPointF(close->width()/2,close->height()/2)).toPoint());
    });
    QTimer::singleShot(1000, Qt::PreciseTimer, &fixture, &Fixture::empty);
    QTimer::singleShot(500, Qt::PreciseTimer, &fixture, [&fixture] {
        // Deliberately leave Qt's own hover state untouched. Native crossings
        // must control hiding even if Qt reports stale hover.
        fixture.leaveDock();
        fixture.setAutoHide(true);
    });
    bool hideMotionOk = false;
    QTimer::singleShot(800, Qt::PreciseTimer, window, [window] { window->setProperty("dockVisible", false); });
    QTimer::singleShot(860, Qt::PreciseTimer, window, [window, &hideMotionOk] {
        auto *content = visualChild(window->contentItem(),"dockContent");
        auto *panel = visualChild(window->contentItem(),"dockPanel");
        const auto progress = window->property("slide").toDouble();
        hideMotionOk = content && panel && progress > 0 && progress < 1
            && panel->y() + content->y() < window->height();
    });
    bool hidden = false;
    bool revealed = false;
    QTimer::singleShot(2350, Qt::PreciseTimer, window, [window, &hidden, &fixture] {
        hidden = !window->property("dockVisible").toBool();
        fixture.reachEdge();
    });
    QTimer::singleShot(2500, Qt::PreciseTimer, window, [window, &revealed] {
        // Wait for the edge dwell timer instead of requiring one exact frame.
        // No forced-show callback runs before this bounded wait finishes.
        revealed = QTest::qWaitFor([window] { return window->property("dockVisible").toBool(); },250);
    });
    QTimer::singleShot(2650, Qt::PreciseTimer, &fixture, &Fixture::restore);
    bool dropCopyOk = false;
    QTimer::singleShot(2700, Qt::PreciseTimer, window, [window, &fixture, &dropCopyOk] {
        auto *panel = visualChild(window->contentItem(),"dockPanel");
        if (!panel) return;
        const auto position = panel->mapToScene(QPointF(panel->width()/2,panel->height()/2)).toPoint();
        QMimeData mime;
        mime.setData("text/uri-list", QByteArray());
        QDragEnterEvent enter(position,Qt::CopyAction | Qt::MoveAction,&mime,Qt::LeftButton,Qt::NoModifier);
        QCoreApplication::sendEvent(window,&enter);
        mime.setUrls({QUrl::fromLocalFile("/tmp/dock-drop.desktop")});
        QDropEvent drop(QPointF(position),Qt::CopyAction | Qt::MoveAction,&mime,Qt::LeftButton,Qt::NoModifier);
        drop.setDropAction(Qt::MoveAction);
        QCoreApplication::sendEvent(window,&drop);
        dropCopyOk = enter.isAccepted() && drop.isAccepted() && drop.dropAction() == Qt::CopyAction && fixture.dropCalls == 1;
    });
    bool unobstructedOk = false;
    QTimer::singleShot(2780, Qt::PreciseTimer, window, [window,&fixture,&unobstructedOk] {
        window->setProperty("dockVisible",false);
        fixture.obstructed_ = false;
        emit fixture.dockObstructedChanged();
        unobstructedOk = window->property("dockVisible").toBool() && !fixture.desktopVisible();
        fixture.obstructed_ = true;
        emit fixture.dockObstructedChanged();
    });
    bool desktopShownOk = false;
    QTimer::singleShot(2750, Qt::PreciseTimer, window, [window, &fixture, &desktopShownOk] {
        window->setProperty("dockVisible", false);
        fixture.setDesktopVisible(true);
        desktopShownOk = window->property("dockVisible").toBool();
        fixture.setDesktopVisible(false);
    });
    // Isolate bounce from hover/card transitions. The previous test sampled a
    // single early frame and compared floating-point rectangles exactly while
    // other animations were still settling.
    QRectF fixedPanel, fixedSlot;
    bool bounceOk = false, settledOk = false, decayingBounceOk = false;
    bool moved = false, panelStable = true, slotStable = true, bounceFinished = false;
    double lastOffset = 0;
    QTimer probe, deadline;
    probe.setInterval(10);
    probe.setTimerType(Qt::PreciseTimer);
    deadline.setSingleShot(true);
    deadline.setInterval(2500);
    const auto sameRect = [](const QRectF &a, const QRectF &b) {
        constexpr qreal epsilon = 0.25;
        return std::abs(a.x()-b.x()) <= epsilon && std::abs(a.y()-b.y()) <= epsilon
            && std::abs(a.width()-b.width()) <= epsilon && std::abs(a.height()-b.height()) <= epsilon;
    };
    const auto finish = [&] {
        qInfo("QML checks: warnings=%d maskCalls=%d hidden=%d revealed=%d visible=%d", warnings,
              fixture.maskCalls, hidden, revealed, window->property("dockVisible").toBool());
        qInfo("Drag checks: calls=%d from=%s to=%s", fixture.reorderCalls,
              qPrintable(fixture.movedKey), qPrintable(fixture.targetKey));
        qInfo("Desktop drop checks: copyAction=%d calls=%d",dropCopyOk,fixture.dropCalls);
        qInfo("Desktop visibility checks: shown=%d",desktopShownOk);
        qInfo("Bounce checks: moved=%d panelStable=%d slotStable=%d decayed=%d lastOffset=%.3f settled=%d",
              moved, panelStable, slotStable, decayingBounceOk, lastOffset, settledOk);
        qInfo("Motion checks: uniformIdle=%d hideVisibleMidway=%d bounce=%d settled=%d", uniformIdleOk, hideMotionOk, bounceOk, settledOk);
        app.exit(chooserOk && settingsOk && warnings == 0 && fixture.maskCalls > 0 && hidden && revealed && geometryOk
                 && uniformIdleOk && hideMotionOk && bounceOk && settledOk && decayingBounceOk && dropCopyOk && desktopShownOk && unobstructedOk
                 && fixture.closeCalls == 1 && fixture.closedWindow == 1
                 && fixture.reorderCalls == 1 && fixture.movedKey == "1" && fixture.targetKey == "3"
                 && window->property("dockVisible").toBool() ? 0 : 1);
    };
    const auto stopBounce = [&] {
        if (bounceFinished) return;
        bounceFinished = true;
        probe.stop();
        deadline.stop();
        fixture.setLaunching(false);
        QTimer::singleShot(220, Qt::PreciseTimer, window, [&] {
            auto *tile = visualChild(window->contentItem(),"dockIcon-2");
            settledOk = tile && std::abs(tile->property("bounceOffset").toDouble()) < 0.1;
            finish();
        });
    };
    QObject::connect(&deadline, &QTimer::timeout, &app, stopBounce);
    QObject::connect(&probe, &QTimer::timeout, &app, [&] {
        auto *panel = visualChild(window->contentItem(),"dockPanel");
        auto *tile = visualChild(window->contentItem(),"dockIcon-2");
        if (!panel || !tile) return;
        lastOffset = tile->property("bounceOffset").toDouble();
        moved |= lastOffset < -2;
        panelStable &= sameRect(QRectF(panel->position(),panel->size()),fixedPanel);
        slotStable &= sameRect(QRectF(tile->position(),tile->size()),fixedSlot);
        decayingBounceOk |= tile->property("bounceCount").toInt() >= 1
            && tile->property("bounceAmplitude").toDouble() < 18;
        bounceOk = moved && panelStable && slotStable;
        if (bounceOk && decayingBounceOk) stopBounce();
    });
    QTimer::singleShot(2800, Qt::PreciseTimer, window, [&] {
        fixture.setAutoHide(false);
        fixture.leaveDock();
        window->setProperty("previewKey", "");
        window->setProperty("menuOpen", false);
        fixture.restore();
    });
    QTimer::singleShot(3000, Qt::PreciseTimer, window, [&] {
        auto *panel = visualChild(window->contentItem(),"dockPanel");
        auto *tile = visualChild(window->contentItem(),"dockIcon-2");
        if (!panel || !tile) { qWarning("Bounce fixture was not created"); finish(); return; }
        fixedPanel = QRectF(panel->position(),panel->size());
        fixedSlot = QRectF(tile->position(),tile->size());
        fixture.setLaunching(true);
        probe.start();
        deadline.start();
    });
    return app.exec();
}

#include "qml-smoke.moc"
