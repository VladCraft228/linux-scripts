#include "runtime.h"
#include "dock-rust/src/bridge.cxxqt.h"
#include <QDebug>
#include <QIcon>
#include <QJsonDocument>
#include <QPainter>
#include <QStandardPaths>
static QImage iconImage(const QString &name, const QString &letter) {
  const QIcon icon =
      name.startsWith('/') ? QIcon(name) : QIcon::fromTheme(name);
  if (!icon.isNull())
    return icon.pixmap(128, 128).toImage();
  QImage image(128, 128, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter p(&image);
  p.setRenderHint(QPainter::Antialiasing);
  p.setPen(Qt::NoPen);
  p.setBrush(QColor("#657fbc"));
  p.drawRoundedRect(QRectF(8, 8, 112, 112), 25, 25);
  p.setPen(Qt::white);
  auto f = p.font();
  f.setPixelSize(46);
  p.setFont(f);
  p.drawText(image.rect(), Qt::AlignCenter, letter.left(1).toUpper());
  return image;
}
Runtime::Runtime(QObject *parent)
    : QObject(parent), engine_(std::make_unique<dock::RustEngine>()) {
  refresh_.setSingleShot(true);
  refresh_.setInterval(25);
  fallback_.setInterval(80);
  launch_.setSingleShot(true);
  connect(&refresh_, &QTimer::timeout, this,
          [this] { request({{"op", "refresh"}}); });
  connect(&fallback_, &QTimer::timeout, this,
          [this] { request({{"op", "fallback"}}); });
  connect(&launch_, &QTimer::timeout, this,
          [this] { request({{"op", "timeout"}}); });
  QVariantMap preferences;
  settings_.beginGroup("dock/appearance");
  for (const auto &key : settings_.childKeys())
    preferences[key] = settings_.value(key);
  settings_.endGroup();
  QJsonObject initial{
      {"pins", QJsonArray::fromStringList(
                   settings_.value("dock/pinnedDesktopIds").toStringList())},
      {"imports", QJsonObject::fromVariantMap(
                      settings_.value("dock/importedDesktopFiles").toMap())},
      {"preferences", QJsonObject::fromVariantMap(preferences)},
      {"autoHide", settings_.value("dock/autoHide", true).toBool()}};
  request({{"op", "init"},
           {"config", initial},
           {"importDir",
            QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) +
                "/launchers"},
           {"dataDirs",
            QJsonArray::fromStringList(QStandardPaths::standardLocations(
                QStandardPaths::GenericDataLocation))}});
  if (valid_) {
    notifier_ =
        new QSocketNotifier(state["fd"].toInt(), QSocketNotifier::Read, this);
    connect(notifier_, &QSocketNotifier::activated, this,
            [this] { request({{"op", "events"}}); });
  }
}
Runtime::~Runtime() {
  if (notifier_)
    notifier_->setEnabled(false);
}
QJsonValue Runtime::request(QJsonObject request) {
  if (!valid_ && request.value("op") != "init")
    return false;
  const auto input =
      QString::fromUtf8(QJsonDocument(request).toJson(QJsonDocument::Compact));
  const auto output = engine_->dispatch(input);
  const auto response = QJsonDocument::fromJson(output.toUtf8()).object();
  apply(response);
  return response.value("result");
}
void Runtime::apply(const QJsonObject &response) {
  if (response.value("fatal").toBool()) {
    valid_ = false;
    refresh_.stop();
    fallback_.stop();
    launch_.stop();
    if (notifier_)
      notifier_->setEnabled(false);
  } else
    valid_ = true;
  if (response.contains("error")) {
    const auto text = response["error"].toString();
    qWarning().noquote() << text;
    emit error(text);
  }
  if (!valid_)
    return;
  state = response;
  for (const auto &value : response["images"].toArray()) {
    const auto desc = value.toObject();
    const auto theme = desc["theme"].toString();
    const auto pixels = desc["pixels"].toArray();
    const int w = desc["width"].toInt(), h = desc["height"].toInt();
    QImage image;
    if (QIcon::hasThemeIcon(theme))
      image = iconImage(theme, desc["letter"].toString());
    else if (w > 0 && h > 0 && w <= 1024 && h <= 1024 &&
             pixels.size() == w * h) {
      image = QImage(w, h, QImage::Format_ARGB32);
      for (int i = 0; i < pixels.size(); ++i)
        reinterpret_cast<quint32 *>(image.scanLine(i / w))[i % w] =
            quint32(pixels[i].toDouble());
      image =
          image.scaled(128, 128, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    } else
      image = iconImage(theme, desc["letter"].toString());
    images_[quint32(desc["wid"].toDouble())] = image;
  }
  if (response.contains("config")) {
    config = response["config"].toObject();
    settings_.setValue("dock/pinnedDesktopIds",
                       config["pins"].toVariant().toStringList());
    settings_.setValue("dock/importedDesktopFiles",
                       config["imports"].toObject().toVariantMap());
    settings_.setValue("dock/autoHide", config["autoHide"].toBool());
    const auto preferences = config["preferences"].toObject();
    for (auto it = preferences.begin(); it != preferences.end(); ++it)
      settings_.setValue("dock/appearance/" + it.key(), it.value().toInt());
    settings_.sync();
    if (settings_.status() != QSettings::NoError)
      emit error("Could not save dock settings. Check the configuration "
                 "directory permissions.");
    emit configUpdated();
  }
  if (response.contains("tasks")) {
    tasks = response["tasks"].toArray();
    QSet<quint32> live;
    for (const auto &v : tasks)
      live.insert(quint32(v.toObject()["windowId"].toDouble()));
    for (auto it = images_.begin(); it != images_.end();) {
      if (!live.contains(it.key()))
        it = images_.erase(it);
      else
        ++it;
    }
    emit tasksUpdated();
  }
  if (response.contains("groups")) {
    groups = response["groups"].toArray();
    emit groupsUpdated();
  }
  if (response["refreshNeeded"].toBool() && !refresh_.isActive())
    refresh_.start();
  if (response["fallback"].toBool()) {
    if (!fallback_.isActive())
      fallback_.start();
  } else
    fallback_.stop();
  if (response["launchTimeout"].isDouble())
    launch_.start(qMax(1, response["launchTimeout"].toInt()));
  else
    launch_.stop();
  emit stateUpdated();
}
QImage Runtime::applicationIcon(const QString &key) const {
  for (const auto &v : groups) {
    const auto g = v.toObject();
    if (g["appKey"] == key) {
      for (const auto &name : g["iconNames"].toArray())
        if (QIcon::hasThemeIcon(name.toString()))
          return iconImage(name.toString(), g["appName"].toString());
      return iconImage(g["icon"].toString(), g["appName"].toString());
    }
  }
  return {};
}

QImage Runtime::catalogIcon(const QString &id) {
  const auto e = request({{"op", "catalogIcon"}, {"id", id}}).toObject();
  for (const auto &name : e["iconNames"].toArray())
    if (QIcon::hasThemeIcon(name.toString()))
      return iconImage(name.toString(), e["name"].toString());
  return iconImage(e["icon"].toString(), e["name"].toString());
}
