#pragma once

#include "FireBaseClient.h"
#include "TimeTracking.h"

#define EXPORT extern "C" __declspec(dllexport)

EXPORT void init();
EXPORT bool addProcess(const char* processName);
EXPORT bool deleteProcess(const char* processName);