#include "stdafx.h"
#include "FireBaseClient.h"

FirebaseClient::FirebaseClient(const string& baseUrl) : RestBase(baseUrl) { }

FirebaseClient::FirebaseClient(const string& baseUrl, const string& authentication) : RestBase(baseUrl, authentication) { }

json::value FirebaseClient::getData(const string& searchQuery) {
	uri_builder builder(conversions::to_string_t(searchQuery + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return getDataBase(builder);
}

bool FirebaseClient::deleteData(const string& appName)
{
	uri_builder builder(conversions::to_string_t("apps/" + appName + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return deleteDataBase(builder);
}

bool FirebaseClient::putData(const string& appName)
{
	auto now = std::chrono::system_clock::now();
	std::time_t currentTime = std::chrono::system_clock::to_time_t(now);
	struct tm *time = std::localtime(&currentTime);

	json::value content;
	content[L"duration"] = json::value::number(0);
	content[L"name"] = json::value::string(conversions::to_string_t(appName));
	content[L"launches"] = json::value::number(0);
	content[L"longestSession"] = json::value::number(0);

	json::value day;
	day[L"day"] = json::value::array();
	day[L"day"][time->tm_mday] = json::value::number(0);

	json::value month;
	month[L"month"] = json::value::array();
	month[L"month"][time->tm_mon] = day;

	json::value year;
	year[L"year"] = json::value::array();
	year[L"year"][time->tm_year + 1900] = month;

	content[L"timeline"] = year;

	uri_builder builder(conversions::to_string_t("apps/" + appName + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return putDataBase(builder, content);
}

bool FirebaseClient::putData(const string& url, json::value value) {
	uri_builder builder(conversions::to_string_t(url + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return putDataBase(builder, value);
}

bool FirebaseClient::patchData(const string& appName, const string& key, const json::value& value)
{
	json::value content;
	content[conversions::to_string_t(key)] = value;

	uri_builder builder(conversions::to_string_t("apps/" + appName + ".json"));
	builder.append_query(U("auth"), conversions::to_string_t(getAuthenticationString()));
	return patchDataBase(builder, content);
}

vector<string> FirebaseClient::getProcesses() {
	auto processNames = getData("apps");

	vector<string> processes;
	for (auto iter = processNames.as_object().cbegin(); iter != processNames.as_object().cend(); ++iter) {
		processes.push_back(conversions::to_utf8string(iter->first));
	}

	return processes;
}

bool FirebaseClient::onDataRecieve(RECIEVE_TYPES type, const string& process, int data) {
	switch (type)
	{
		case RestBase::RECIEVE_TYPES::LONGEST_SESSION: {
			return patchData(process, "longestSession", json::value::number(
				max(data, getData("apps/" + process + "/longestSession").as_integer())));
		}

		case RestBase::RECIEVE_TYPES::DURATION: {
			auto duration = getData("apps/" + process + "/duration").as_integer();
			return patchData(process, "duration", json::value::number(duration += data));
		}

		case RestBase::RECIEVE_TYPES::LAUNCHES: {
			int launches = getData("apps/" + process + "/launches").as_integer();
			return patchData(process, "launches", json::value::number(launches += data));
		}

		case RestBase::RECIEVE_TYPES::TIMELINE: {
			auto today = chrono::system_clock::now();
			time_t currentTime = chrono::system_clock::to_time_t(today);
			struct tm *time = localtime(&currentTime);

			const string& key = "/timeline/year/"
				+ to_string(time->tm_year + 1900) + "/month/" + to_string(time->tm_mon)
				+ "/day/" + to_string(time->tm_mday);

			const string& url = "apps/" + process + key;

			const json::value& todayDataJson = getData(url);

			if (!wcscmp(todayDataJson.serialize().c_str(), L"null")) {
				return putData(url, json::value::number(1));
			}
			else {
				auto todayData = todayDataJson.as_integer();
				return patchData(process, key, todayData += data);
			}
		}
	default:
		break;
	}

	return true;
}