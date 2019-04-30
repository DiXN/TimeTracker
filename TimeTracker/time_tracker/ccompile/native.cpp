#include "native.h"

const char* query_file_info(const char* path) {
	DWORD  verHandle = 0;
	LPVOID lpBuffer;
	DWORD  verSize = GetFileVersionInfoSize(path, &verHandle);

	if (verSize != NULL) {
		LPSTR verData = new CHAR[verSize];

		if (GetFileVersionInfo(path, verHandle, verSize, verData)) {
			DWORD* pTransTable;
			UINT nQuerySize;
			auto langCharSet = 0;

			if (VerQueryValue(verData, "\\VarFileInfo\\Translation", (void**)&pTransTable, &nQuerySize))
				langCharSet = MAKELONG(HIWORD(pTransTable[0]), LOWORD(pTransTable[0]));

			char queryString[MAX_PATH];
			sprintf_s(queryString, "\\StringFileInfo\\%08lx\\%s", langCharSet, "ProductName");
			UINT size = 0;

			if (VerQueryValue(verData, queryString, &lpBuffer, &size)) {
				if (size)
					return static_cast<LPSTR>(lpBuffer);
			}
		}

		delete[] verData;
	}

	return {};
}

bool is_process_running(const char* process) {
	PROCESSENTRY32 pe;
	HANDLE handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
	pe.dwSize = sizeof(PROCESSENTRY32);
	HANDLE processHandle;

	if (!Process32First(handle, &pe)) {
		CloseHandle(handle);
		return false;
	}

	do {
		if (!lstrcmpi(pe.szExeFile, process))
			return true;

	} while (Process32Next(handle, &pe));

	CloseHandle(handle);

	return false;
}