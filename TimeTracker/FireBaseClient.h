#pragma once
#include "RestBase.h"
#include "TimeTracking.h"
#include <codecvt>

class FirebaseClient : public RestBase {

	public:
		FirebaseClient(const string& baseUrl);
		FirebaseClient(const string& baseUrl, const string& authentication);

		json::value getData(const string& searchQuery) override sealed;
		bool deleteData(const string& appName) override sealed;
		bool putData(const string& appName) override sealed;
		bool patchData(const string& appName, const string& key, const json::value& value) override sealed;
};