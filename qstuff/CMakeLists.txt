cmake_minimum_required(VERSION 3.0)
project(qstuff)

find_package(Qt5 REQUIRED COMPONENTS Gui Widgets Network Charts)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)

add_subdirectory(src)
