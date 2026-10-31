cmake_minimum_required(VERSION 3.21)

project(OpenForensicImager VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
include(GNUInstallDirs)

find_package(Qt6 6.5 REQUIRED COMPONENTS
    Core Gui Widgets Quick QuickControls2 QuickWidgets Qml Test)
qt_standard_project_setup()

set(OFI_RUST_PROFILE "debug" CACHE STRING "Cargo profile for native bindings")
set_property(CACHE OFI_RUST_PROFILE PROPERTY STRINGS debug release)
if(OFI_RUST_PROFILE STREQUAL "release")
    set(OFI_RUST_DEFAULT_TARGET_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release")
    set(OFI_CARGO_PROFILE_ARGS --release)
else()
    set(OFI_RUST_DEFAULT_TARGET_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug")
    set(OFI_CARGO_PROFILE_ARGS)
endif()
set(OFI_RUST_TARGET_DIR "${OFI_RUST_DEFAULT_TARGET_DIR}" CACHE PATH
    "Directory containing libnative_bindings.so")
set(OFI_NATIVE_LIBRARY "${OFI_RUST_TARGET_DIR}/libnative_bindings.so")
set(OFI_NATIVE_WORKER "${OFI_RUST_TARGET_DIR}/ofi-parser-worker")
set(OFI_NATIVE_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/native-bindings/include")

add_custom_target(ofi_native_bindings ALL
    COMMAND cargo build --locked ${OFI_CARGO_PROFILE_ARGS} -p native-bindings
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../.."
    BYPRODUCTS "${OFI_NATIVE_LIBRARY}" "${OFI_NATIVE_WORKER}"
    COMMENT "Building the Rust native bindings (${OFI_RUST_PROFILE})")

set(OFI_APP_SOURCES
    src/main.cpp
    src/abi_client.cpp
    src/abi_client.h
    src/models.cpp
    src/models.h
    src/app_controller.cpp
    src/app_controller.h
    src/main_window.cpp
    src/main_window.h)

qt_add_executable(openforensic-imager ${OFI_APP_SOURCES})

target_include_directories(openforensic-imager PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
    "${OFI_NATIVE_INCLUDE}")

target_compile_options(openforensic-imager PRIVATE -Wall -Wextra -Wpedantic -Werror)

target_link_libraries(openforensic-imager PRIVATE
    Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Quick Qt6::QuickControls2 Qt6::QuickWidgets Qt6::Qml
    "${OFI_NATIVE_LIBRARY}")

add_dependencies(openforensic-imager ofi_native_bindings)

set_target_properties(openforensic-imager PROPERTIES
    BUILD_RPATH "$ORIGIN"
    INSTALL_RPATH "$ORIGIN/../${CMAKE_INSTALL_LIBDIR}/openforensic-imager")
add_custom_command(TARGET openforensic-imager POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OFI_NATIVE_LIBRARY}"
        "$<TARGET_FILE_DIR:openforensic-imager>/libnative_bindings.so"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OFI_NATIVE_WORKER}"
        "$<TARGET_FILE_DIR:openforensic-imager>/ofi-parser-worker")

qt_add_resources(openforensic-imager "qml"
    PREFIX "/qml"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../ui/qml"
    FILES
        ../../ui/qml/App.qml
        ../../ui/qml/CasePanel.qml
        ../../ui/qml/EmptyState.qml
        ../../ui/qml/EvidenceTree.qml
        ../../ui/qml/FileList.qml
        ../../ui/qml/PropertiesPanel.qml
        ../../ui/qml/TaskPanel.qml
        ../../ui/qml/Theme.qml
        ../../ui/qml/ViewerTabs.qml)

add_executable(openforensic-imager-tests
    tests/tst_models.cpp
    src/abi_client.cpp
    src/abi_client.h
    src/models.cpp
    src/models.h
    src/app_controller.cpp
    src/app_controller.h)

# The tests assert on the shipped QML sources, so compile the same resource
# set into the test binary under its own prefix.
qt_add_resources(openforensic-imager-tests "qml"
    PREFIX "/qml"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../ui/qml"
    FILES
        ../../ui/qml/App.qml
        ../../ui/qml/CasePanel.qml
        ../../ui/qml/EmptyState.qml
        ../../ui/qml/EvidenceTree.qml
        ../../ui/qml/FileList.qml
        ../../ui/qml/PropertiesPanel.qml
        ../../ui/qml/TaskPanel.qml
        ../../ui/qml/Theme.qml
        ../../ui/qml/ViewerTabs.qml)

target_include_directories(openforensic-imager-tests PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
    "${OFI_NATIVE_INCLUDE}")

target_compile_options(openforensic-imager-tests PRIVATE -Wall -Wextra -Wpedantic -Werror)

if(CMAKE_BUILD_TYPE STREQUAL "Release")
    # Build-directory paths become a transient RPATH before install. Excluding
    # the GNU build ID keeps that path from changing the packaged executable.
    target_link_options(openforensic-imager PRIVATE "LINKER:--build-id=none")
    target_link_options(openforensic-imager-tests PRIVATE "LINKER:--build-id=none")
endif()

target_link_libraries(openforensic-imager-tests PRIVATE
    Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Quick Qt6::QuickControls2 Qt6::QuickWidgets Qt6::Qml Qt6::Test
    "${OFI_NATIVE_LIBRARY}")

add_dependencies(openforensic-imager-tests ofi_native_bindings)
set_target_properties(openforensic-imager-tests PROPERTIES
    BUILD_RPATH "$ORIGIN")
add_custom_command(TARGET openforensic-imager-tests POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OFI_NATIVE_LIBRARY}"
        "$<TARGET_FILE_DIR:openforensic-imager-tests>/libnative_bindings.so"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OFI_NATIVE_WORKER}"
        "$<TARGET_FILE_DIR:openforensic-imager-tests>/ofi-parser-worker")

install(TARGETS openforensic-imager
    RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}")
install(PROGRAMS "${OFI_NATIVE_LIBRARY}"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/openforensic-imager")
install(PROGRAMS "${OFI_NATIVE_WORKER}"
    DESTINATION "${CMAKE_INSTALL_BINDIR}")
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/../../packaging/common/openforensic-imager.desktop"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/applications"
    RENAME "io.github.x0r909.OpenForensicImager.desktop")
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/../../packaging/common/openforensic-imager.svg"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps"
    RENAME "io.github.x0r909.OpenForensicImager.svg")
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/../../packaging/common/openforensic-imager.metainfo.xml"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/metainfo"
    RENAME "io.github.x0r909.OpenForensicImager.metainfo.xml")
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/../../README.md"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../packaging/common/THIRD_PARTY.md"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../packaging/debian/copyright"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/doc/openforensic-imager")

enable_testing()
add_test(NAME openforensic-imager-tests COMMAND openforensic-imager-tests)
add_test(NAME openforensic-imager-self-test
         COMMAND openforensic-imager --self-test)
set_tests_properties(openforensic-imager-self-test PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
add_test(NAME openforensic-imager-self-test-dark
         COMMAND openforensic-imager --dark --self-test)
set_tests_properties(openforensic-imager-self-test-dark PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

add_test(NAME openforensic-imager-qml-tests
         COMMAND /usr/lib/qt6/bin/qmltestrunner
                 -input "${CMAKE_CURRENT_SOURCE_DIR}/../../ui/qml/tests"
                 -import "${CMAKE_CURRENT_SOURCE_DIR}/../../ui/qml")
set_tests_properties(openforensic-imager-qml-tests PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
