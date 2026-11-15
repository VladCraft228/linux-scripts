import QtQuick
import QtQuick.Window

Window {
    id: root
    width: 1000
    height: 656
    visible: false
    color: "transparent"
    title: "Linux Scripts Dock — application dock"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.WindowDoesNotAcceptFocus

    readonly property var preferences: applicationModel.preferences
    readonly property color normalBorderColor: Qt.rgba(((preferences.borderColor >> 16) & 255)/255,
        ((preferences.borderColor >> 8) & 255)/255, (preferences.borderColor & 255)/255, preferences.borderOpacity/100)
    property int settingsPage: 0
    onSettingsPageChanged: {
        if(settingsPage===0) colorTarget="borderColor"
        else if(settingsPage===3) colorTarget="indicatorActiveColor"
    }
    function rgbColor(value) { return Qt.rgba(((value>>16)&255)/255,((value>>8)&255)/255,(value&255)/255,1) }
    property string colorTarget: "borderColor"
    readonly property int pickerColor: preferences[colorTarget]
    readonly property color backgroundColor: Qt.rgba(((preferences.backgroundColor >> 16) & 255)/255,
        ((preferences.backgroundColor >> 8) & 255)/255,(preferences.backgroundColor & 255)/255,preferences.opacity/100)
    onColorTargetChanged: { pickerHueMemory=0; pickerSaturationMemory=0 }
    property real pickerHueMemory: 0
    property real pickerSaturationMemory: 0
    readonly property var pickerHsv: {
        const c=pickerColor
        const r=((c>>16)&255)/255, g=((c>>8)&255)/255, b=(c&255)/255
        const v=Math.max(r,g,b), low=Math.min(r,g,b), d=v-low
        let h=0
        if(d>0) h=(v===r ? (g-b)/d : v===g ? (b-r)/d+2 : (r-g)/d+4)/6
        return {h:v===0?pickerHueMemory:(h+1)%1, s:v===0?pickerSaturationMemory:d/v, v:v}
    }
    function setPickerHsv(h,s,v) {
        pickerHueMemory=h; pickerSaturationMemory=Math.max(0,Math.min(1,s))
        const c=Qt.hsva(h,Math.max(0,Math.min(1,s)),Math.max(0,Math.min(1,v)),1)
        applicationModel.setPreference(colorTarget,(Math.round(c.r*255)<<16)|(Math.round(c.g*255)<<8)|Math.round(c.b*255))
    }
    function setPickerHex(text) {
        if (!/^#?[0-9a-fA-F]{6}$/.test(text)) return false
        applicationModel.setPreference(colorTarget, parseInt(text.replace("#", ""),16))
        return true
    }
    property int bottomOffset: preferences.bottomOffset
    readonly property real zoomMaximum: 1 + preferences.magnification / 100
    function openApplicationChooser() {
        menuOpen=false; previewKey=""
        appChooser.entries=applicationModel.availableApplications()
        appSearch.text="";appChooser.notice=""
        appChooser.show();appChooser.raise();appChooser.requestActivate();appSearch.forceActiveFocus()
    }
    function openAbout() {
        menuOpen=false; previewKey=""
        aboutWindow.show(); aboutWindow.raise(); aboutWindow.requestActivate()
    }
    function openSettings() {
        menuOpen = false
        previewKey = ""
        settingsWindow.show()
        settingsWindow.raise()
        settingsWindow.requestActivate()
    }
    readonly property real effectiveOffset: Math.min(bottomOffset, Math.max(0, height - 250))
    readonly property real pointerX: pointerMonitor.point.position.x
    // Native X11 crossings are authoritative: a QML hover handler can retain
    // hover after the pointer leaves a dynamically shaped transparent window.
    readonly property bool hovering: windowModel.pointerInside
    property bool dockVisible: true
    property real slide: dockVisible ? 0 : 1
    // Move only far enough to clear the lower edge, not a whole window height.
    readonly property real hideTravel: effectiveOffset + Math.max(84, baseSize * zoomMaximum + 36) + 12
    property bool menuOpen: false
    property string menuKey: ""
    property var menuRows: []
    property string hoverTitle: ""
    property string message: ""
    property string dragKey: ""
    property int dragFrom: -1
    property int dragTo: -1
    property real dragX: 0
    property real dragY: 0
    property string dragIcon: ""
    readonly property bool dragging: dragKey.length > 0
    property bool externalDragActive: false
    property int dropPosition: 0
    onExternalDragActiveChanged: reconsiderHide()
    property string previewCandidate: ""
    property string previewKey: ""
    property var previewItems: []
    property int previewTotal: 0
    readonly property bool previewOpen: previewKey.length > 0 && previewItems.length > 0
    property int layoutRevision: 0
    property int sizeRevision: 0
    // Repeater.itemAt() does not notify bindings when a delegate becomes ready.
    // Explicit invalidation also accounts for the last delegate's animation.
    readonly property var iconBounds: {
        const revision = layoutRevision
        let left = width / 2
        let right = width / 2
        let top = baseline - baseSize
        let first = true
        for (let i = 0; i < icons.count; ++i) {
            const item = icons.itemAt(i)
            if (!item) continue
            left = first ? item.x : Math.min(left, item.x)
            right = first ? item.x + item.width : Math.max(right, item.x + item.width)
            top = first ? item.y : Math.min(top, item.y)
            first = false
        }
        return {left: left, right: right, top: top}
    }
    readonly property int gap: preferences.iconSpacing
    readonly property real baseSize: Math.max(22, Math.min(preferences.iconSize,
        (width - 150 - Math.max(0, icons.count - 1) * gap) / Math.max(1, icons.count)))
    readonly property real baseWidth: icons.count * baseSize + Math.max(0, icons.count - 1) * gap
    readonly property real baseline: height - effectiveOffset - 26
    property real totalWidth: {
        const revision = sizeRevision
        let result = Math.max(0, icons.count - 1) * gap
        for (let i = 0; i < icons.count; ++i) {
            const item = icons.itemAt(i)
            if (item) result += item.width
        }
        return result
    }
    Behavior on slide {
        NumberAnimation {
            duration: root.preferences.animationDuration
            easing.type: Easing.InOutCubic
        }
    }
    onSlideChanged: scheduleMask()
    onHoveringChanged: {
        if (!hovering) hoverTitle = ""
        if (!hovering && previewOpen) previewClose.restart()
        reconsiderHide()
    }
    onDraggingChanged: reconsiderHide()
    onPreviewOpenChanged: { scheduleMask(); reconsiderHide() }

    function cancelDrag() {
        dragKey = ""
        dragFrom = -1
        dragTo = -1
    }
    function visualSlot(index) {
        if (!dragging || dragFrom === dragTo) return index
        if (index === dragFrom) return dragTo
        if (dragFrom < dragTo && index > dragFrom && index <= dragTo) return index - 1
        if (dragFrom > dragTo && index >= dragTo && index < dragFrom) return index + 1
        return index
    }
    function moveDrag(x, y) {
        dragX = x
        dragY = y
        let count = 0
        for (let i = 0; i < icons.count; ++i) {
            if (icons.itemAt(i) && icons.itemAt(i).pinned) ++count
        }
        const slot = Math.round((x - (width - baseWidth) / 2 - baseSize / 2) / (baseSize + gap))
        dragTo = Math.max(0, Math.min(count - 1, slot))
    }
    function finishDrag() {
        const target = icons.itemAt(dragTo)
        const source = dragKey
        const key = target ? target.appKey : ""
        const inPanel = dragY >= panel.y - 45 && dragY <= panel.y + panel.height + 25
            && dragX >= panel.x && dragX <= panel.x + panel.width
        cancelDrag()
        if (inPanel && key.length > 0) applicationModel.reorderPinned(source, key)
    }
    function insertionAt(x) {
        let position = 0
        for (let i = 0; i < icons.count; ++i) {
            const item = icons.itemAt(i)
            if (item && item.pinned && x > item.x + item.width / 2) ++position
        }
        return position
    }
    function showPreview(key) {
        if (dragging || menuOpen || !hovering) return
        const items = applicationModel.previewWindows(key)
        previewTotal = items.length
        previewItems = items.slice(0, 4)
        previewKey = items.length > 0 ? key : ""
        scheduleMask()
    }
    function targetSize(i, key) {
        if (dragging || externalDragActive) return baseSize
        const idle = baseSize
        if (menuOpen || !dockVisible) return idle
        // Popup motion must not move the zoom peak to unrelated dock icons.
        if (previewOpen) return key === previewKey ? baseSize * zoomMaximum : idle
        if (!hovering) return idle
        const center = (width - baseWidth) / 2 + i * (baseSize + gap) + baseSize / 2
        const distance = Math.abs(pointerX - center)
        const radius = (baseSize + gap) * 1.4
        const wave = distance < radius ? (1 + Math.cos(Math.PI * distance / radius)) / 2 : 0
        const weight = Math.pow(wave, 2.5)
        return Math.max(idle, baseSize * (1 + preferences.magnification / 100 * weight))
    }
    function itemLeft(i) {
        if (dragging) return (width - baseWidth) / 2 + visualSlot(i) * (baseSize + gap)
        let left = (width - totalWidth) / 2
        for (let j = 0; j < i; ++j) {
            const item = icons.itemAt(j)
            if (item) left += item.width + gap
        }
        return left
    }
    function previewAnchor() {
        const revision = layoutRevision
        let center = width / 2
        let top = panel.y
        for (let i = 0; i < icons.count; ++i) {
            const item = icons.itemAt(i)
            if (!item) continue
            top = Math.min(top, item.y)
            if (item.appKey === previewKey) center = item.x + item.width / 2
        }
        return {center: center, top: top}
    }
    function openMenu(key) {
        previewTimer.stop()
        previewKey = ""
        menuKey = key
        menuRows = applicationModel.menu(key)
        menuOpen = true
        dockVisible = true
        hideTimer.stop()
    }
    function reconsiderHide() {
        if (!applicationModel.autoHide || !windowModel.dockObstructed) {
            hideTimer.stop()
            dockVisible = true
        } else if (hovering || menuOpen || dragging || externalDragActive || previewOpen || settingsWindow.visible || aboutWindow.visible || appChooser.visible) hideTimer.stop()
        else hideTimer.restart()
    }
    function scheduleMask() { Qt.callLater(updateMask) }
    function updateMask() {
        // The normal resting footprint stays fixed during zoom and sliding.
        windowModel.setDockArea(panel.x,panel.y,panel.width,panel.height)
        const shift = dockContent.y
        const rectangles = [{x: edge.x, y: edge.y, width: edge.width, height: edge.height}]
        if (slide < 1) {
            rectangles.push({x: panel.x, y: panel.y + shift, width: panel.width, height: panel.height})
            for (let i = 0; i < icons.count; ++i) {
                const item = icons.itemAt(i)
                if (item) rectangles.push({x: item.x, y: item.y + shift, width: item.width, height: item.height + 14})
            }
            if (previewOpen) {
                rectangles.push({x: previewPanel.x, y: previewPanel.y + shift, width: previewPanel.width, height: previewPanel.height})
                // A small input bridge lets the pointer travel from icon to preview.
                rectangles.push({x: previewPanel.x, y: previewPanel.y + previewPanel.height + shift,
                                 width: previewPanel.width, height: Math.max(0, panel.y - previewPanel.y - previewPanel.height)})
            }
            if (menuOpen) rectangles.push({x: menuPanel.x, y: menuPanel.y + shift, width: menuPanel.width, height: menuPanel.height})
        }
        windowModel.setInputRegion(rectangles)
    }
    onDockVisibleChanged: windowModel.setDockHidden(!dockVisible)
    onWidthChanged: scheduleMask()
    onHeightChanged: scheduleMask()
    onMenuOpenChanged: { scheduleMask(); reconsiderHide() }
    Component.onCompleted: {
        scheduleMask()
        windowModel.setDockHidden(!dockVisible)
        reconsiderHide()
    }

    Timer {
        id: hideTimer
        interval: root.preferences.hideDelay
        onTriggered: {
            if (applicationModel.autoHide && windowModel.dockObstructed && !root.hovering && !root.menuOpen && !root.dragging && !root.externalDragActive && !root.previewOpen && !settingsWindow.visible && !aboutWindow.visible && !appChooser.visible) root.dockVisible = false
        }
    }
    Timer { id: previewTimer; interval: 400; onTriggered: root.showPreview(root.previewCandidate) }
    Timer { id: previewClose; interval: 280; onTriggered: { if (!root.hovering || !previewHover.hovered) root.previewKey = "" } }
    Timer { id: edgeTimer; interval: 120; onTriggered: root.dockVisible = true }
    Timer { id: messageTimer; interval: 5000; onTriggered: root.message = "" }
    HoverHandler {
        id: pointerMonitor
        target: null
    }
    Connections {
        target: windowModel
        function onDockObstructedChanged() { root.reconsiderHide() }
        function onDesktopVisibleChanged() { root.reconsiderHide() }
        function onEdgeHoveredChanged() {
            if (windowModel.edgeHovered) edgeTimer.restart()
            else edgeTimer.stop()
        }
    }
    Connections {
        target: applicationModel
        function onAutoHideChanged() { root.reconsiderHide() }
        function onModelReset() {
            root.menuOpen = false
            root.previewKey = ""
            root.previewCandidate = ""
            previewTimer.stop()
            root.cancelDrag()
        }
        function onDataChanged() {
            if (root.menuOpen) root.menuRows = applicationModel.menu(root.menuKey)
            if (root.previewOpen) {
                const items = applicationModel.previewWindows(root.previewKey)
                root.previewTotal = items.length
                root.previewItems = items.slice(0, 4)
                root.scheduleMask()
            }
        }
        function onLaunchError(text) {
            root.message = text
            root.dockVisible = true
            messageTimer.restart()
        }
    }
    // Only this narrow centered strip catches input when the dock is hidden.
    Item {
        id: edge
        x: (root.width - width) / 2
        y: root.height - 3
        width: Math.max(180, Math.min(root.width, root.baseWidth + 32))
        height: 3
        onWidthChanged: root.scheduleMask()
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            onClicked: { root.dockVisible = true; root.menuOpen = false }
        }
    }
    Item {
        id: dockContent
        objectName: "dockContent"
        width: root.width
        height: root.height
        y: root.slide * root.hideTravel
        visible: root.slide < 1
        Rectangle {
            id: panel
            objectName: "dockPanel"
            width: Math.max(180, root.baseWidth + 32)
            height: root.baseSize + 36
            x: (root.width - width) / 2
            y: root.baseline - root.baseSize - 10
            radius: Math.min(root.preferences.cornerRadius, height / 2, width / 2)
            color: root.backgroundColor
            border.width: root.preferences.borderEnabled ? root.preferences.borderWidth : (root.externalDragActive ? 1 : 0)
            border.color: root.externalDragActive ? "#9bc9ff" : root.normalBorderColor
            onXChanged: root.scheduleMask()
            onYChanged: root.scheduleMask()
            onWidthChanged: root.scheduleMask()
            onHeightChanged: root.scheduleMask()
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: function(mouse) {
                    if (mouse.button === Qt.RightButton) root.openMenu("")
                    else { root.menuOpen = false; root.previewKey = "" }
                }
            }
        }
        Text {
            anchors.centerIn: panel
            visible: icons.count === 0
            text: "Open an application to pin it"
            color: "#e4e9f2"
            font.pixelSize: 12
        }
        DropArea {
            id: launcherDrop
            objectName: "desktopDropArea"
            z: 10
            x: panel.x
            y: Math.min(panel.y, root.baseline - root.baseSize * zoomMaximum - 12)
            width: panel.width
            height: panel.y + panel.height - y
            onEntered: function(drag) {
                // Native XDND sources may not expose URL data until the actual
                // drop. Accept the advertised format here; validate files later.
                const valid = drag.formats.indexOf("text/uri-list") >= 0 || drag.hasUrls
                drag.accepted = valid
                if (valid) {
                    root.externalDragActive = true
                    root.dockVisible = true
                    root.previewKey = ""
                    root.menuOpen = false
                    previewTimer.stop()
                    root.dropPosition = root.insertionAt(x + drag.x)
                    hideTimer.stop()
                }
            }
            onPositionChanged: function(drag) {
                drag.accepted = root.externalDragActive
                if (root.externalDragActive) root.dropPosition = root.insertionAt(x + drag.x)
            }
            onExited: root.externalDragActive = false
            onDropped: function(drop) {
                let action = Qt.IgnoreAction
                if (drop.supportedActions & Qt.CopyAction) action = Qt.CopyAction
                else if (drop.supportedActions & Qt.LinkAction) action = Qt.LinkAction
                if (action === Qt.IgnoreAction) {
                    root.message = "The drag source must allow copying or linking a launcher."
                    messageTimer.restart()
                    drop.accepted = false
                } else {
                    // Read the native selection while the drop event is alive.
                    // Some file managers expose only text/uri-list here.
                    let urls = []
                    const raw = drop.getDataAsString("text/uri-list")
                    if (raw) {
                        const lines = raw.split(/\r?\n/)
                        for (let i = 0; i < lines.length; ++i) {
                            const line = lines[i].trim()
                            if (line.length && line[0] !== "#") urls.push(line)
                        }
                    }
                    if (!urls.length) {
                        for (let i = 0; i < drop.urls.length; ++i) urls.push(String(drop.urls[i]))
                    }
                    if (applicationModel.pinDesktopFiles(urls,root.dropPosition)) drop.accept(action)
                    else drop.accepted = false
                }
                root.externalDragActive = false
            }
        }
        Rectangle {
            visible: root.externalDragActive
            width: 3
            height: root.baseSize + 16
            radius: 1
            x: (root.width - root.baseWidth) / 2 + root.dropPosition * (root.baseSize + root.gap) - root.gap / 2
            y: root.baseline - root.baseSize - 8
            color: "#9bc9ff"
        }
        Repeater {
            id: icons
            model: applicationModel
            onItemAdded: { ++root.sizeRevision; ++root.layoutRevision; root.scheduleMask() }
            onItemRemoved: { ++root.sizeRevision; ++root.layoutRevision; root.scheduleMask() }
            delegate: Item {
                id: tile
                objectName: "dockIcon-" + appKey
                required property int index
                required property string appKey
                required property string appName
                required property string iconUrl
                required property bool appActive
                required property int runningCount
                required property bool pinned
                required property bool launching
                property bool bounceReady: false
                property real bounceOffset: 0
                property real bounceAmplitude: 18
                property int bounceCount: 0
                function updateBounce() {
                    if (!bounceReady) return
                    if (launching && root.dockVisible && !root.dragging) {
                        if (!launchBounce.running && !bounceSettle.running) {
                            bounceCount = 0
                            bounceAmplitude = 18
                            launchBounce.start()
                        }
                    } else {
                        launchBounce.stop()
                        if (bounceOffset !== 0) bounceSettle.restart()
                    }
                }
                onLaunchingChanged: updateBounce()
                Component.onCompleted: { bounceReady = true; updateBounce() }
                Connections {
                    target: root
                    function onDockVisibleChanged() { tile.updateBounce() }
                    function onDraggingChanged() { tile.updateBounce() }
                }
                SequentialAnimation {
                    id: launchBounce
                    // One bounded 400 ms cycle; the model owns the launch deadline.
                    NumberAnimation {
                        target: tile; property: "bounceOffset"; to: -tile.bounceAmplitude
                        duration: 170; easing.type: Easing.OutQuad
                    }
                    NumberAnimation {
                        target: tile; property: "bounceOffset"; to: 0
                        duration: 230; easing.type: Easing.InQuad
                    }
                    onFinished: {
                        ++tile.bounceCount
                        if (tile.launching && root.dockVisible && !root.dragging && tile.bounceCount < 30) {
                            tile.bounceAmplitude = Math.max(4, tile.bounceAmplitude * 0.84)
                            restart()
                        }
                    }
                }
                NumberAnimation {
                    id: bounceSettle
                    target: tile; property: "bounceOffset"; to: 0
                    duration: 130; easing.type: Easing.OutCubic
                    onFinished: {
                        if (tile.launching && root.dockVisible && !root.dragging) tile.updateBounce()
                    }
                }
                width: root.targetSize(index, appKey)
                height: width
                x: root.itemLeft(index)
                y: root.baseline - height
                opacity: root.dragging && root.dragKey === appKey ? 0.25 : 1
                Behavior on x { enabled: root.dragging; NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
                Behavior on width { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
                onXChanged: { ++root.layoutRevision; root.scheduleMask() }
                onYChanged: { ++root.layoutRevision; root.scheduleMask() }
                onWidthChanged: { ++root.sizeRevision; ++root.layoutRevision; root.scheduleMask() }
                Image {
                    objectName: "launchImage-" + tile.appKey
                    anchors.fill: parent
                    transform: Translate { y: tile.bounceOffset }
                    source: tile.iconUrl
                    sourceSize.width: 128
                    sourceSize.height: 128
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    asynchronous: false
                    cache: false
                }
                StatusMark {
                    active: tile.appActive
                    visible: tile.runningCount > 0 && root.preferences.indicatorStyle !== 4
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: parent.height + 6
                    objectName: "activeIndicator-" + tile.appKey
                }
                Rectangle {
                    visible: tile.runningCount > 1
                    width: countText.implicitWidth + 8
                    height: 17
                    radius: 8
                    anchors.right: parent.right
                    color: "#ee343e54"
                    Text { id: countText; anchors.centerIn: parent; text: tile.runningCount; color: "white"; font.pixelSize: 11 }
                }
                MouseArea {
                    id: tileMouse
                    objectName: "dockMouse-" + tile.appKey
                    cursorShape: root.dragging ? Qt.ClosedHandCursor : Qt.PointingHandCursor
                    anchors.fill: parent
                    hoverEnabled: true
                    preventStealing: true
                    acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                    property real startX: 0
                    property real startY: 0
                    property bool wasDragging: false
                    onEntered: {
                        if (root.previewOpen && root.previewKey !== tile.appKey) root.previewKey = ""
                        root.hoverTitle = tile.appName
                        root.previewCandidate = tile.appKey
                        previewClose.stop()
                        previewTimer.restart()
                    }
                    onExited: {
                        root.hoverTitle = ""
                        previewTimer.stop()
                        previewClose.restart()
                    }
                    onPressed: function(mouse) {
                        const point = tile.mapToItem(root.contentItem, mouse.x, mouse.y)
                        startX = point.x
                        startY = point.y
                        wasDragging = false
                        previewTimer.stop()
                        root.previewKey = ""
                    }
                    onPositionChanged: function(mouse) {
                        if (!(pressedButtons & Qt.LeftButton) || !tile.pinned) return
                        const point = tile.mapToItem(root.contentItem, mouse.x, mouse.y)
                        if (!root.dragging && Math.abs(point.x - startX) + Math.abs(point.y - startY) > 10) {
                            root.dragFrom = tile.index
                            root.dragTo = tile.index
                            root.dragIcon = tile.iconUrl
                            root.dragKey = tile.appKey
                            root.dockVisible = true
                            wasDragging = true
                        }
                        if (root.dragging) root.moveDrag(point.x, point.y)
                    }
                    onReleased: {
                        if (root.dragging) root.finishDrag()
                    }
                    onCanceled: { root.cancelDrag(); wasDragging = true }
                    onClicked: function(mouse) {
                        if (wasDragging) return
                        if (mouse.button === Qt.RightButton) root.openMenu(tile.appKey)
                        else {
                            root.menuOpen = false
                            root.previewKey = ""
                            if (mouse.button === Qt.MiddleButton) applicationModel.perform("launch", tile.appKey, 0)
                            else applicationModel.activate(tile.appKey)
                        }
                    }
                }
            }
        }
        Rectangle {
            visible: root.hovering && root.hoverTitle.length > 0 && !root.menuOpen && !root.previewOpen && !root.dragging
            width: Math.min(root.width - 20, titleLabel.implicitWidth + 24)
            height: 28
            x: (root.width - width) / 2
            y: panel.y - 65
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
            id: previewPanel
            objectName: "windowPreview"
            visible: root.previewOpen
            width: Math.min(root.width - 24, root.previewItems.length * 184 + 16)
            height: 170
            x: Math.max(8, Math.min(root.width - width - 8, root.previewAnchor().center - width / 2))
            y: Math.max(4, root.previewAnchor().top - height - 12)
            radius: 14
            color: "#fa283244"
            border.color: "#607e8aa0"
            onWidthChanged: root.scheduleMask()
            onXChanged: root.scheduleMask()
            onYChanged: root.scheduleMask()
            HoverHandler {
                id: previewHover
                target: null
                onHoveredChanged: {
                    if (hovered) previewClose.stop()
                    else previewClose.restart()
                }
            }
            Row {
                x: 8
                y: 8
                spacing: 8
                Repeater {
                    model: root.previewItems
                    delegate: Rectangle {
                        id: windowCard
                        required property var modelData
                        width: (previewPanel.width - 16 - Math.max(0, root.previewItems.length - 1) * 8) / Math.max(1, root.previewItems.length)
                        height: 126
                        radius: 8
                        color: previewMouse.containsMouse ? "#52617d" : "#303c52"
                        border.width: modelData.active ? 2 : 0
                        border.color: "#8abcff"
                        Image {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 12
                            width: 64
                            height: 64
                            source: windowCard.modelData.iconUrl
                            sourceSize.width: 128
                            sourceSize.height: 128
                            cache: false
                            fillMode: Image.PreserveAspectFit
                        }
                        Text {
                            x: 8; y: 84; width: parent.width - 16; height: 30
                            text: windowCard.modelData.title
                            color: "white"
                            font.pixelSize: 11
                            elide: Text.ElideRight
                            verticalAlignment: Text.AlignVCenter
                            horizontalAlignment: Text.AlignHCenter
                        }
                        MouseArea {
                            id: previewMouse
                            cursorShape: Qt.PointingHandCursor
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: {
                                applicationModel.perform("window", root.previewKey, windowCard.modelData.wid)
                                root.previewKey = ""
                            }
                        }
                        Rectangle {
                            width: 24
                            height: 24
                            x: parent.width - width - 4
                            y: 4
                            radius: 6
                            color: closeMouse.containsMouse ? "#c64b59" : "#506079"
                            Text { anchors.centerIn: parent; text: "×"; color: "white"; font.pixelSize: 18 }
                            MouseArea {
                                id: closeMouse
                                objectName: "closeWindow-" + windowCard.modelData.wid
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                hoverEnabled: true
                                onClicked: applicationModel.perform("close", root.previewKey, windowCard.modelData.wid)
                            }
                        }
                    }
                }
            }
            Text {
                visible: root.previewTotal <= 1
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 10
                anchors.horizontalCenter: parent.horizontalCenter
                text: "Click to activate · × to close"
                color: "#b7c5db"
                font.pixelSize: 10
            }
            Rectangle {
                visible: root.previewTotal > 1
                height: 24
                width: parent.width - 16
                x: 8
                y: parent.height - height - 8
                radius: 6
                color: closeAllMouse.containsMouse ? "#944756" : "#3c4961"
                Text {
                    anchors.centerIn: parent
                    text: "Close all windows (" + root.previewTotal + ")" + (root.previewTotal > 4 ? " · more windows in menu" : "")
                    color: "white"
                    font.pixelSize: 11
                }
                MouseArea {
                    id: closeAllMouse
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onClicked: {
                        const key = root.previewKey
                        root.previewKey = ""
                        applicationModel.perform("closeAll", key, 0)
                    }
                }
            }
        }
        Rectangle {
            id: menuPanel
            visible: root.menuOpen
            width: Math.min(330, root.width - 20)
            height: Math.min(menuList.contentHeight + 12, Math.max(80, panel.y - 20))
            x: (root.width - width) / 2
            y: panel.y - height - 12
            radius: 12
            color: "#fa30394a"
            border.color: "#607e8aa0"
            onHeightChanged: root.scheduleMask()
            onYChanged: root.scheduleMask()
            Flickable {
                id: menuList
                anchors.fill: parent
                anchors.margins: 6
                contentWidth: width
                contentHeight: menuColumn.height
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: menuColumn
                    width: menuList.width
                    Repeater {
                        model: root.menuRows
                        delegate: Rectangle {
                            required property var modelData
                            width: menuColumn.width
                            height: 34
                            radius: 6
                            color: rowMouse.containsMouse && modelData.enabled ? "#637393" : "transparent"
                            Text {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                                text: modelData.label
                                color: modelData.enabled ? "white" : "#8892a5"
                                font.pixelSize: 12
                            }
                            MouseArea {
                                id: rowMouse
                                cursorShape: parent.modelData.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                                anchors.fill: parent
                                hoverEnabled: true
                                enabled: parent.modelData.enabled
                                onClicked: {
                                    const action = parent.modelData.action
                                    const wid = parent.modelData.wid
                                    const key = root.menuKey
                                    root.menuOpen = false
                                    if (action === "quit") Qt.quit()
                                    else if (action === "settings") root.openSettings()
                                    else if (action === "about") root.openAbout()
                                    else if (action === "addApplication") root.openApplicationChooser()
                                    else applicationModel.perform(action, key, wid)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Image {
        visible: root.dragging
        z: 100
        x: root.dragX - width / 2
        y: root.dragY - height / 2
        width: root.baseSize
        height: width
        source: root.dragIcon
        sourceSize.width: 128
        sourceSize.height: 128
        fillMode: Image.PreserveAspectFit
        opacity: 0.85
    }
    Rectangle {
        visible: root.message.length > 0
        width: Math.min(600, root.width - 20)
        height: messageText.implicitHeight + 20
        x: (root.width - width) / 2
        y: 10
        radius: 10
        color: "#ee663c42"
        Text {
            id: messageText
            width: parent.width - 24
            anchors.centerIn: parent
            text: root.message
            wrapMode: Text.WordWrap
            color: "white"
            font.pixelSize: 12
        }
    }
    Window {
        id: settingsWindow
        objectName: "dockSettings"
        width: 420; height: 630
        minimumWidth: 420; maximumWidth: 420
        minimumHeight: 630; maximumHeight: 630
        title: "Dock Settings"
        color: "#252b36"
        visible: false
        onVisibleChanged: root.reconsiderHide()
        property bool resetPending: false
        onClosing: resetPending=false
        Column {
            x: 24; y: 20; spacing: 16
            Text { text: "Dock Settings"; color: "white"; font.pixelSize: 22 }
            Row {
                spacing: 6
                Repeater {
                    model: ["Panel","Icons","Behavior","Indicators"]
                    Rectangle {
                        required property string modelData
                        required property int index
                        objectName: "settingsTab-" + index
                        width: 88.5; height: 34; radius: 6
                        color: root.settingsPage===index ? "#526d98" : "#354158"
                        Text { anchors.centerIn: parent; text: parent.modelData; color: "white"; font.pixelSize: 11 }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                            onClicked: { root.settingsPage=parent.index; settingsWindow.resetPending=false; settingsViewport.contentY=0 } }
                    }
                }
            }
        }
        Flickable {
            id: settingsViewport
            objectName: "settingsViewport"
            x: 0; y: 112; width: parent.width; height: parent.height-198
            clip: true
            contentWidth: width
            contentHeight: settingsColumn.y + settingsColumn.height + 24
            flickableDirection: Flickable.VerticalFlick
            boundsBehavior: Flickable.StopAtBounds
        Column {
            id: settingsColumn
            x: 24; y: 22; spacing: 17
            Text { text: ["Panel appearance","Icon magnification","Showing and hiding","Running and active applications"][root.settingsPage]; color: "#b6c2d4"; font.pixelSize: 12 }
            SettingSlider { label: "Icon size"; settingKey: "iconSize"; visible: root.settingsPage===1; minimum: 24; maximum: 80; unit: " px" }
            SettingSlider { label: "Icon spacing"; settingKey: "iconSpacing"; visible: root.settingsPage===1; minimum: 0; maximum: 32; unit: " px" }
            SettingSlider { label: "Magnification"; settingKey: "magnification"; visible: root.settingsPage===1; minimum: 0; maximum: 100; unit: "%" }
            SettingSlider { label: "Background opacity"; settingKey: "opacity"; visible: root.settingsPage===0; minimum: 0; maximum: 100; unit: "%" }
            SettingSlider { label: "Bottom offset"; settingKey: "bottomOffset"; visible: root.settingsPage===0; minimum: 0; maximum: 160; unit: " px" }
            SettingSlider { label: "Show / hide animation"; settingKey: "animationDuration"; visible: root.settingsPage===2; minimum: 60; maximum: 500; step: 10; unit: " ms" }
            SettingSlider { label: "Border opacity"; settingKey: "borderOpacity"; visible: root.settingsPage===0; minimum: 0; maximum: 100; unit: "%" }
            SettingSlider { label: "Border width"; settingKey: "borderWidth"; visible: root.settingsPage===0; minimum: 1; maximum: 8; unit: " px" }
            SettingSlider { label: "Corner radius"; settingKey: "cornerRadius"; visible: root.settingsPage===0; minimum: 0; maximum: 60; unit: " px" }
            SettingSlider { label: "Hide delay"; settingKey: "hideDelay"; visible: root.settingsPage===2; minimum: 0; maximum: 3000; step: 50; unit: " ms" }
            Column {
                visible: root.settingsPage===3
                width: 372; spacing: 12
                Text { text: "Indicator style"; color: "#e3e9f3"; font.pixelSize: 13 }
                Flow {
                    width: 372; spacing: 6
                    Repeater {
                        model: ["Pill", "Dot", "Line", "Square", "Hidden"]
                        Rectangle {
                            required property string modelData
                            required property int index
                            width: 68; height: 30; radius: 5
                            color: root.preferences.indicatorStyle===index ? "#526d98" : "#354158"
                            Text { anchors.centerIn: parent; text: parent.modelData; color: "white"; font.pixelSize: 12 }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                onClicked: applicationModel.setPreference("indicatorStyle",parent.index) }
                        }
                    }
                }
                Row {
                    spacing: 16; height: 24
                    Text { text: "Active"; color: "#b6c2d4"; font.pixelSize: 12 }
                    StatusMark { active: true; visible: root.preferences.indicatorStyle!==4; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: "Running"; color: "#b6c2d4"; font.pixelSize: 12 }
                    StatusMark { active: false; visible: root.preferences.indicatorStyle!==4; anchors.verticalCenter: parent.verticalCenter }
                }
                Text { text: "Colors apply to running apps and the active window separately."; width: 372; wrapMode: Text.WordWrap; color: "#b6c2d4"; font.pixelSize: 12 }
            }
            Column {
                visible: root.settingsPage===0 || root.settingsPage===3
                width: 372; spacing: 10
                Text { text: "Color palette"; color: "#e3e9f3"; font.pixelSize: 13 }
                Row {
                    spacing: 8
                    Repeater {
                        model: root.settingsPage===3 ? [{label:"Active window",key:"indicatorActiveColor"},{label:"Running app",key:"indicatorRunningColor"}] : [{label:"Border",key:"borderColor"},{label:"Background",key:"backgroundColor"}]
                        Rectangle {
                            required property var modelData
                            objectName: "colorTarget-" + modelData.key
                            width: 182; height: 32; radius: 5
                            color: root.colorTarget===modelData.key ? "#526d98" : "#354158"
                            Text { anchors.centerIn: parent; text: parent.modelData.label; color: "white"; font.pixelSize: 13 }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                onClicked: root.colorTarget=parent.modelData.key }
                        }
                    }
                }
                Item {
                    width: 372; height: 184
                    Canvas {
                        id: colorWheel
                        objectName: "borderColorWheel"
                        width: 176; height: 176
                        onPaint: {
                            const ctx=getContext("2d"), cx=width/2, cy=height/2, r=width/2-2
                            ctx.clearRect(0,0,width,height)
                            for(let i=0;i<360;++i) {
                                const a=i*Math.PI/180, next=(i+1.5)*Math.PI/180
                                const gradient=ctx.createRadialGradient(cx,cy,0,cx,cy,r)
                                gradient.addColorStop(0,"white")
                                gradient.addColorStop(1,Qt.hsva(i/360,1,1,1))
                                ctx.fillStyle=gradient
                                ctx.beginPath();ctx.moveTo(cx,cy);ctx.arc(cx,cy,r,a,next);ctx.closePath();ctx.fill()
                            }
                        }
                        Rectangle {
                            width: 12; height: 12; radius: 6; color: "transparent"
                            border.color: "#111827"; border.width: 2
                            x: parent.width/2 + Math.cos(root.pickerHsv.h*2*Math.PI)*root.pickerHsv.s*(parent.width/2-2)-6
                            y: parent.height/2 + Math.sin(root.pickerHsv.h*2*Math.PI)*root.pickerHsv.s*(parent.height/2-2)-6
                            Rectangle { anchors.fill: parent; anchors.margins: 2; radius: 4; color: "transparent"; border.color: "white"; border.width: 1 }
                        }
                        MouseArea {
                            anchors.fill: parent; cursorShape: Qt.CrossCursor; preventStealing: true
                            function choose(x,y) {
                                const dx=x-width/2,dy=y-height/2
                                const h=(Math.atan2(dy,dx)/(2*Math.PI)+1)%1
                                root.setPickerHsv(h,Math.min(1,Math.sqrt(dx*dx+dy*dy)/(width/2-2)),root.pickerHsv.v)
                            }
                            onPressed: function(mouse) { choose(mouse.x,mouse.y) }
                            onPositionChanged: function(mouse) { if(pressed) choose(mouse.x,mouse.y) }
                        }
                    }
                    Column {
                        x: 198; width: 174; spacing: 12
                        Text { text: "Preview"; color: "#e3e9f3"; font.pixelSize: 12 }
                        Rectangle { width: 110; height: 66; radius: 8; color: root.colorTarget==="backgroundColor" ? Qt.hsva(root.pickerHsv.h,root.pickerHsv.s,root.pickerHsv.v,1) : "#252b36"; border.width: root.preferences.borderWidth; border.color: root.colorTarget==="backgroundColor" ? root.normalBorderColor : Qt.hsva(root.pickerHsv.h,root.pickerHsv.s,root.pickerHsv.v,1) }
                        Text { text: "Brightness: " + Math.round(root.pickerHsv.v*100) + "%"; color: "#e3e9f3"; font.pixelSize: 12 }
                        Rectangle {
                            width: 174; height: 22; radius: 4
                            gradient: Gradient { orientation: Gradient.Horizontal; GradientStop { position: 0; color: "black" } GradientStop { position: 1; color: Qt.hsva(root.pickerHsv.h,root.pickerHsv.s,1,1) } }
                            Rectangle { width: 3; height: 26; y: -2; x: root.pickerHsv.v*(parent.width-3); color: "white" }
                            MouseArea {
                                anchors.fill: parent; cursorShape: Qt.PointingHandCursor; preventStealing: true
                                function choose(x) { root.setPickerHsv(root.pickerHsv.h,root.pickerHsv.s,x/width) }
                                onPressed: function(mouse) { choose(mouse.x) }
                                onPositionChanged: function(mouse) { if(pressed) choose(mouse.x) }
                            }
                        }
                    }
                }
                Row {
                    spacing: 10
                    Repeater {
                        model: ["#ffffff", "#111827", "#78b3ff", "#ff667a", "#65d6a6", "#f5c76b", "#b99aff", "#00d4ff"]
                        Rectangle {
                            required property string modelData
                            width: 32; height: 26; radius: 5; color: modelData
                            border.width: root.pickerColor === parseInt(modelData.slice(1),16) ? 3 : 1
                            border.color: "#9aa9bf"
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                onClicked: root.setPickerHex(parent.modelData) }
                        }
                    }
                }
                Rectangle {
                    width: 372; height: 30; radius: 5; color: "#354158"
                    TextInput {
                        id: borderHex
                        objectName: "borderHexInput"
                        anchors.fill: parent; anchors.margins: 6
                        color: acceptableInput ? "white" : "#ff8999"
                        font.pixelSize: 13; selectByMouse: true
                        text: "#" + root.pickerColor.toString(16).padStart(6,"0")
                        validator: RegularExpressionValidator { regularExpression: /#[0-9a-fA-F]{6}/ }
                        onEditingFinished: root.setPickerHex(text)
                    }
                }
            }
            Rectangle {
                visible: root.settingsPage===0
                objectName: "borderToggle"
                width: 372; height: 32; radius: 6
                color: borderMouse.containsMouse ? "#485773" : "#354158"
                Text { anchors.centerIn: parent; text: "Border: " + (root.preferences.borderEnabled ? "On" : "Off"); color: "white" }
                MouseArea { id: borderMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                    onClicked: applicationModel.setPreference("borderEnabled", root.preferences.borderEnabled ? 0 : 1) }
            }
            Rectangle {
                width: 372; height: 32; radius: 6
                visible: root.settingsPage===2
                color: autoHideMouse.containsMouse ? "#485773" : "#354158"
                Text { anchors.centerIn: parent; text: "Intelligent hide: " + (applicationModel.autoHide ? "On" : "Off"); color: "white" }
                MouseArea { id: autoHideMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                    onClicked: applicationModel.autoHide = !applicationModel.autoHide }
            }
        }
        }
        Column {
            x: 24; y: parent.height-78; width: 372; spacing: 6
            Text { text: settingsWindow.resetPending ? "Reset appearance and behavior? Pinned apps are kept." : "Changes are saved automatically."; width: 372; wrapMode: Text.WordWrap; color: "#b6c2d4"; font.pixelSize: 11 }
            Row {
                spacing: 8
                Rectangle {
                    objectName: "settingsResetButton"
                    width: settingsWindow.resetPending ? 270 : 372; height: 32; radius: 6
                    color: settingsWindow.resetPending ? "#884b54" : "#354158"
                    Text { anchors.centerIn: parent; text: settingsWindow.resetPending ? "Confirm reset" : "Restore defaults…"; color: "white"; font.pixelSize: 12 }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if(!settingsWindow.resetPending) settingsWindow.resetPending=true
                            else { applicationModel.resetPreferences(); settingsWindow.resetPending=false }
                        }
                    }
                }
                Rectangle {
                    objectName: "settingsResetCancel"
                    visible: settingsWindow.resetPending; width: 94; height: 32; radius: 6; color: "#354158"
                    Text { anchors.centerIn: parent; text: "Cancel"; color: "white"; font.pixelSize: 12 }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: settingsWindow.resetPending=false }
                }
            }
        }
    }
    Window {
        id: aboutWindow
        objectName: "dockAbout"
        width: 440; height: 490
        minimumWidth: 440; maximumWidth: 440
        minimumHeight: 490; maximumHeight: 490
        title: "About Linux Scripts Dock"
        color: "#252b36"
        visible: false
        onVisibleChanged: root.reconsiderHide()
        Column {
            x: 24; y: 24; width: 392; spacing: 16
            Text { text: "Linux Scripts Dock"; font.pixelSize: 25; color: "white" }
            Text { text: "Version " + (Qt.application.version || "development"); font.pixelSize: 12; color: "#b6c2d4" }
            Text { text: "A customizable application dock for Linux.\nRust engine · Qt Quick / QML · XFCE / X11"; width: 392; wrapMode: Text.WordWrap; font.pixelSize: 13; color: "#dce5f3" }
            Rectangle { width: 392; height: 1; color: "#4b586c" }
            Text { text: "Created by Vladyslav Havryliuk (VladCraft228).\nDeveloped with assistance from ChatGPT (OpenAI)."; width: 392; wrapMode: Text.WordWrap; font.pixelSize: 13; color: "#dce5f3" }
            Text { text: "Part of linux-scripts — practical Linux desktop tools."; width: 392; wrapMode: Text.WordWrap; font.pixelSize: 12; color: "#b6c2d4" }
            Column {
                spacing: 8
                AboutLink { label: "GitHub repository & other tools"; target: "https://github.com/VladCraft228/linux-scripts" }
                AboutLink { label: "Documentation & installation"; target: "https://github.com/VladCraft228/linux-scripts#readme" }
                AboutLink { label: "Report a bug / suggest a feature"; target: "https://github.com/VladCraft228/linux-scripts/issues/new" }
                AboutLink { label: "Author on GitHub"; target: "https://github.com/VladCraft228" }
            }
        }
    }
    Window {
        id: appChooser
        objectName: "applicationChooser"
        width: 460; height: 540
        minimumWidth: 460; minimumHeight: 400
        title: "Add application to Dock"
        color: "#252b36"; visible: false
        property var entries: []
        property string notice: ""
        readonly property var filtered: {
            const term=appSearch.text.trim().toLowerCase()
            return entries.filter(function(app) {return !term || app.name.toLowerCase().indexOf(term)>=0 || app.id.toLowerCase().indexOf(term)>=0})
        }
        onVisibleChanged: root.reconsiderHide()
        Text { x: 20; y: 20; text: "Add application"; color: "white"; font.pixelSize: 22 }
        Rectangle {
            x: 20; y: 62; width: parent.width-40; height: 36; radius: 6; color: "#354158"
            Text { anchors.verticalCenter: parent.verticalCenter; x: 10; text: "Search installed applications…"; color: "#9eacc3"; visible: !appSearch.text.length }
            TextInput {
                id: appSearch; objectName: "applicationSearch"
                anchors.fill: parent; anchors.margins: 9; color: "white"; font.pixelSize: 13; selectByMouse: true
            }
        }
        ListView {
            id: appList; objectName: "availableApplicationList"
            x: 20; y: 110; width: parent.width-40; height: parent.height-164
            clip: true; spacing: 5; model: appChooser.filtered
            delegate: Rectangle {
                required property var modelData
                width: appList.width; height: 58; radius: 6
                color: appMouse.containsMouse ? "#465774" : "#303a4d"
                Image { x: 9; y: 11; width: 36; height: 36; sourceSize.width: 48; sourceSize.height: 48; source: "image://catalog/"+encodeURIComponent(parent.modelData.id) }
                Column {
                    x: 56; y: 10; width: parent.width-134; spacing: 4
                    Text { width: parent.width; text: modelData.name; elide: Text.ElideRight; color: "white"; font.pixelSize: 13 }
                    Text { width: parent.width; text: modelData.id; elide: Text.ElideRight; color: "#a8b6ce"; font.pixelSize: 10 }
                }
                Text { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: parent.modelData.pinned ? "Pinned" : "+ Add"; color: parent.modelData.pinned ? "#a8b6ce" : "#b9d8ff"; font.pixelSize: 12 }
                MouseArea {
                    id: appMouse; anchors.fill: parent; hoverEnabled: true
                    enabled: !parent.modelData.pinned; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: {
                        const app=parent.modelData
                        if(applicationModel.pinApplication(app.id)) {
                            appChooser.notice="Added: "+app.name
                            appChooser.entries=applicationModel.availableApplications()
                        } else appChooser.notice="Could not add this launcher. See the Dock notification."
                    }
                }
            }
        }
        Text { anchors.centerIn: appList; text: "No matching applications"; color: "#b6c2d4"; visible: appList.count===0 }
        Text { x: 20; y: parent.height-42; width: parent.width-40; text: appChooser.notice || "Choose an application to pin it without launching it."; wrapMode: Text.WordWrap; color: "#b6c2d4"; font.pixelSize: 12 }
    }
    component AboutLink: Rectangle {
        required property string label
        required property url target
        width: 392; height: 32; radius: 5
        color: linkMouse.containsMouse ? "#485b78" : "#354158"
        Text { anchors.centerIn: parent; text: parent.label; color: "#b9d8ff"; font.pixelSize: 12 }
        MouseArea { id: linkMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
            onClicked: Qt.openUrlExternally(parent.target) }
    }
    component StatusMark: Rectangle {
        property bool active: false
        width: root.preferences.indicatorStyle===2 ? (active?24:14) : root.preferences.indicatorStyle===0 ? (active?22:5) : (active?8:5)
        height: root.preferences.indicatorStyle===2 ? 3 : root.preferences.indicatorStyle===0 ? (active?6:4) : width
        radius: root.preferences.indicatorStyle===3 ? 0 : height/2
        color: root.rgbColor(active ? root.preferences.indicatorActiveColor : root.preferences.indicatorRunningColor)
        border.width: active && root.preferences.indicatorStyle===0 ? 1 : 0
        border.color: Qt.lighter(color,1.5)
    }
    component SettingSlider: Item {
        required property string label
        required property string settingKey
        objectName: "setting-" + settingKey
        required property int minimum
        required property int maximum
        property int step: 1
        property string unit: ""
        readonly property int value: root.preferences[settingKey]
        width: 372; height: 43
        Text { text: parent.label; color: "#e3e9f3"; font.pixelSize: 13 }
        Text { anchors.right: parent.right; text: parent.value + parent.unit; color: "#a9ceff"; font.pixelSize: 13 }
        Rectangle { x: 0; y: 29; width: parent.width; height: 4; radius: 2; color: "#596477" }
        Rectangle { x: 0; y: 29; width: (parent.value-parent.minimum)/(parent.maximum-parent.minimum)*parent.width; height: 4; radius: 2; color: "#80b7ff" }
        Rectangle { x: (parent.value-parent.minimum)/(parent.maximum-parent.minimum)*(parent.width-12); y: 25; width: 12; height: 12; radius: 6; color: "white" }
        MouseArea {
            anchors.fill: parent; anchors.topMargin: 19
            cursorShape: Qt.PointingHandCursor
            function updateValue(x) {
                const slider = parent
                const fraction = Math.max(0,Math.min(1,x/width))
                applicationModel.setPreference(slider.settingKey,Math.round((slider.minimum + fraction*(slider.maximum-slider.minimum))/slider.step)*slider.step)
            }
            onPressed: function(mouse) { updateValue(mouse.x) }
            onPositionChanged: function(mouse) { if (pressed) updateValue(mouse.x) }
        }
    }

}
