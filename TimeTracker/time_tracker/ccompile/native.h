#include "stdafx.h"
#include <iostream>
#include <windows.h>
#include <shlwapi.h>
#include <tlhelp32.h>
#include <Psapi.h>
#include <vector>
#include <array>

#pragma comment(lib, "Version.lib")

#ifdef __cplusplus
extern "C"
{
#endif

using namespace std;

const char* query_file_info(const char* path);
bool is_process_running(const char* process);

#ifdef __cplusplus
}
#endif