import QtQuick
import QtQuick.Window

Window {
    id: root
    width: 1000
    height: 180
    visible: false
    color: "transparent"
    title: "Linux Scripts Dock — technical prototype"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.WindowDoesNotAcceptFocus

    readonly property real pointerX: pointerMonitor.point.position.x
    readonly property bool hovering: pointerMonitor.hovered
    property bool menuOpen: false
    property string hoverTitle: ""
    readonly property int gap: 10
    readonly property real baseSize: Math.max(22, Math.min(48,
        (width - 150 - Math.max(0, icons.count - 1) * gap) / Math.max(1, icons.count)))
    readonly property real baseWidth: icons.count * baseSize + Math.max(0, icons.count - 1) * gap
    readonly property real baseline: height - 26
    property real totalWidth: {
        let result = Math.max(0, icons.count - 1) * gap
        for (let i = 0; i < icons.count; ++i) {
            const item = icons.itemAt(i)
            if (item) result += item.width
        }
        return result
    }

    function targetSize(i) {
        if (!hovering || menuOpen) return baseSize
        const center = (width - baseWidth) / 2 + i * (baseSize + gap) + baseSize / 2
        const distance = Math.abs(pointerX - center)
        const radius = 125
        const weight = distance < radius ? (1 + Math.cos(Math.PI * distance / radius)) / 2 : 0
        return baseSize * (1 + 0.65 * weight)
    }

    function itemLeft(i) {
        let left = (width - totalWidth) / 2
        for (let j = 0; j < i; ++j) {
            const item = icons.itemAt(j)
            if (item) left += item.width + gap
        }
        return left
    }

    // Coalesce geometry changes from animated items into one mask update.
    function scheduleMask() { Qt.callLater(updateMask) }
    function updateMask() {
        const rectangles = [{x: panel.x, y: panel.y, width: panel.width, height: panel.height}]
        for (let i = 0; i < icons.count; ++i) {
            const item = icons.itemAt(i)
            if (item) rectangles.push({x: item.x, y: item.y, width: item.width, height: item.height + 14})
        }
        if (menuOpen) rectangles.push({x: quitButton.x, y: quitButton.y, width: quitButton.width, height: quitButton.height})
        // Tooltip is visual only and does not intercept mouse input.
        windowModel.setInputRegion(rectangles)
    }

    onWidthChanged: scheduleMask()
    onMenuOpenChanged: scheduleMask()
    Component.onCompleted: scheduleMask()

    Rectangle {
        id: panel
        width: Math.max(180, root.totalWidth + 32)
        height: 76
        x: (root.width - width) / 2
        y: root.height - height - 8
        radius: 22
        color: "#d92a303c"
        border.width: 1
        border.color: "#60ffffff"
        onXChanged: root.scheduleMask()
        onWidthChanged: root.scheduleMask()
    }

    HoverHandler {
        id: pointerMonitor
        target: null
        onHoveredChanged: if (!hovered) root.hoverTitle = ""
    }

    MouseArea {
        id: pointerArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.RightButton
        onClicked: root.menuOpen = !root.menuOpen
    }

    Text {
        anchors.centerIn: panel
        visible: icons.count === 0
        text: "Open an application"
        color: "#e4e9f2"
        font.pixelSize: 13
    }

    Repeater {
        id: icons
        model: windowModel
        onItemAdded: root.scheduleMask()
        onItemRemoved: root.scheduleMask()
        delegate: Item {
            id: tile
            required property int index
            required property int windowId
            required property string windowTitle
            required property string iconUrl
            required property bool windowActive
            width: root.targetSize(index)
            height: width
            x: root.itemLeft(index)
            y: root.baseline - height
            Behavior on width { NumberAnimation { duration: 100; easing.type: Easing.OutCubic } }
            onXChanged: root.scheduleMask()
            onYChanged: root.scheduleMask()
            onWidthChanged: root.scheduleMask()

            Image {
                anchors.fill: parent
                source: tile.iconUrl
                sourceSize.width: 128
                sourceSize.height: 128
                fillMode: Image.PreserveAspectFit
                smooth: true
                asynchronous: false
                cache: false
            }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                y: parent.height + 6
                width: tile.windowActive ? 14 : 5
                height: 4
                radius: 2
                color: tile.windowActive ? "#ffffff" : "#8d9aae"
            }
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onEntered: root.hoverTitle = tile.windowTitle
                onExited: root.hoverTitle = ""
                onClicked: function(mouse) {
                    if (mouse.button === Qt.RightButton) root.menuOpen = !root.menuOpen
                    else {
                        root.menuOpen = false
                        windowModel.activate(tile.windowId)
                    }
                }
            }
        }
    }

    Rectangle {
        id: tooltip
        visible: root.hovering && root.hoverTitle.length > 0 && !root.menuOpen
        width: Math.min(root.width - 20, titleLabel.implicitWidth + 24)
        height: 28
        x: (root.width - width) / 2
        y: 10
        radius: 8
        color: "#ee242a36"
        Text {
            id: titleLabel
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            text: root.hoverTitle
            color: "white"
            font.pixelSize: 12
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
        }
    }

    Rectangle {
        id: quitButton
        visible: root.menuOpen
        width: 180
        height: 36
        x: (root.width - width) / 2
        y: 8
        radius: 9
        color: quitArea.containsMouse ? "#ff535f79" : "#ff30394a"
        Text { anchors.centerIn: parent; text: "Quit dock prototype"; color: "white"; font.pixelSize: 13 }
        MouseArea { id: quitArea; anchors.fill: parent; hoverEnabled: true; onClicked: Qt.quit() }
    }
}
