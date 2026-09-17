cmake_minimum_required(VERSION 3.14)
project(runner LANGUAGES CXX)

# Define the application target. To change its name, change BINARY_NAME in the
# top-level CMakeLists.txt, not the value here, or `flutter run` will no longer
# work.
#
# Any new source files that you add to the application should be added here.
add_executable(${BINARY_NAME} WIN32
  "flutter_window.cpp"
  "main.cpp"
  "utils.cpp"
  "win32_window.cpp"
  "${FLUTTER_MANAGED_DIR}/generated_plugin_registrant.cc"
  "Runner.rc"
  "runner.exe.manifest"
)

# Apply the standard set of build settings. This can be removed for applications
# that need different build settings.
apply_standard_settings(${BINARY_NAME})

# Add preprocessor definitions for the build version.
target_compile_definitions(${BINARY_NAME} PRIVATE "FLUTTER_VERSION=\"${FLUTTER_VERSION}\"")
target_compile_definitions(${BINARY_NAME} PRIVATE "FLUTTER_VERSION_MAJOR=${FLUTTER_VERSION_MAJOR}")
target_compile_definitions(${BINARY_NAME} PRIVATE "FLUTTER_VERSION_MINOR=${FLUTTER_VERSION_MINOR}")
target_compile_definitions(${BINARY_NAME} PRIVATE "FLUTTER_VERSION_PATCH=${FLUTTER_VERSION_PATCH}")
target_compile_definitions(${BINARY_NAME} PRIVATE "FLUTTER_VERSION_BUILD=${FLUTTER_VERSION_BUILD}")

# Disable Windows macros that collide with C++ standard library functions.
target_compile_definitions(${BINARY_NAME} PRIVATE "NOMINMAX")

# Add dependency libraries and include directories. Add any application-specific
# dependencies here.
target_link_libraries(${BINARY_NAME} PRIVATE flutter flutter_wrapper_app)
target_link_libraries(${BINARY_NAME} PRIVATE "dwmapi.lib")
target_include_directories(${BINARY_NAME} PRIVATE "${CMAKE_SOURCE_DIR}")

# Run the Flutter tool portions of the build. This must not be removed.
add_dependencies(${BINARY_NAME} flutter_assemble)

# === VaultSync 核心引擎（Rust）===
# cargo 构建 vault-core 动态库并在链接后拷贝进产物目录；cargo 缺失时跳过
# （顶层 CMakeLists 已警告），Dart 侧 FFI 回退 Stub。
if(CARGO_EXECUTABLE)
  set(VAULT_CORE_DLL "${NATIVE_WORKSPACE_DIR}/target/release/vault_core.dll")
  # 链接后事件：cargo 构建（工作目录=workspace 根）+ 拷贝 DLL 进产物目录。
  add_custom_command(TARGET ${BINARY_NAME} POST_BUILD
    COMMAND ${CARGO_EXECUTABLE} build --release
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
      "${VAULT_CORE_DLL}" "$<TARGET_FILE_DIR:${BINARY_NAME}>"
    WORKING_DIRECTORY "${NATIVE_WORKSPACE_DIR}"
    COMMENT "Building & bundling vault_core.dll")
endif()
