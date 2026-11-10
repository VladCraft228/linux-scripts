cmake_minimum_required(VERSION 3.21)
project(linux_scripts_dock VERSION 0.1.0 LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Qml Quick)
find_package(PkgConfig REQUIRED)
pkg_check_modules(XCB REQUIRED IMPORTED_TARGET xcb xcb-shape)
qt_add_executable(linux-scripts-dock
    src/main.cpp
    src/platform/x11/windowmodel.cpp
    src/platform/x11/windowmodel.h
)
qt_add_resources(linux-scripts-dock "ui" PREFIX "/" FILES qml/Main.qml)
target_link_libraries(linux-scripts-dock PRIVATE
    Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick PkgConfig::XCB)
target_compile_options(linux-scripts-dock PRIVATE -Wall -Wextra -Wpedantic)
install(TARGETS linux-scripts-dock RUNTIME DESTINATION bin)
include(CTest)
if(BUILD_TESTING)
    qt_add_executable(dock-qml-smoke tests/qml-smoke.cpp)
    qt_add_resources(dock-qml-smoke "test_ui" PREFIX "/" FILES qml/Main.qml)
    target_link_libraries(dock-qml-smoke PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)
    add_test(NAME dock-qml-smoke COMMAND dock-qml-smoke)
    set_tests_properties(dock-qml-smoke PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
endif()
