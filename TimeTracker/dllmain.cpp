#pragma once
#include "stdafx.h"
#include "dllmain.h"

HANDLE g_hDllHandle = 0;

BOOL WINAPI DllMain(HINSTANCE hInstance, DWORD dwReason, LPVOID)
{
	g_hDllHandle = hInstance;
	DisableThreadLibraryCalls((HMODULE)hInstance);
}