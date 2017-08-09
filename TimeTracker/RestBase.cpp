#include "stdafx.h"
#include "RestBase.h"

RestBase::RestBase(const string& baseUrl) : baseUrl(baseUrl), hasAuthentication(false),
	httpClient(conversions::to_string_t(baseUrl)) { }

RestBase::RestBase(const string& baseUrl, const string& authentication) : baseUrl(baseUrl),
	authentication(authentication), hasAuthentication(true), httpClient(conversions::to_string_t(baseUrl)) { }

const string& RestBase::getBaseUrl() {
	return baseUrl;
}

const string& RestBase::getAuthenticationString() {
	return authentication;
}

bool RestBase::getHasAuthentication() {
	return hasAuthentication;
}

const st_time RestBase::getStartTime() {
	return startTime;
}

void RestBase::setStartTime(const st_time& startTime) {
	this->startTime = startTime;
}

json::value RestBase::getDataBase(uri_builder& builder)
{
	json::value jsonValue = json::value();

	httpClient.request(methods::GET, builder.to_string()).then([&jsonValue](http_response response)
	{
		if (response.status_code() == status_codes::OK) {
			response.extract_json().then([&jsonValue](json::value jVal) {
				jsonValue = jVal;
			}).wait();
		}
		else {
			jsonValue = json::value::null();
		}

	}).wait();

	return jsonValue;
}

bool RestBase::deleteDataBase(uri_builder& builder)
{
	bool returnVal = false;

	httpClient.request(methods::DEL, builder.to_string()).then([&returnVal](http_response response) {
		if (response.status_code() == status_codes::OK) {
			returnVal = true;
		}
	}).wait();

	return returnVal;
}

bool RestBase::putDataBase(uri_builder& builder, const json::value& content)
{
	bool returnVal = false;

	httpClient.request(methods::PUT, builder.to_string(),
		content.serialize().c_str(), L"application/json").then([&returnVal](http_response response) {
		if (response.status_code() == status_codes::OK) {
			returnVal = true;
		}
	}).wait();

	return returnVal;
}

bool RestBase::patchDataBase(uri_builder& builder, const json::value& content)
{
	bool returnVal = false;

	httpClient.request(methods::PATCH, builder.to_string(),
		content.serialize().c_str(), L"application/json").then([&returnVal](http_response response) {
		if (response.status_code() == status_codes::OK) {
			returnVal = true;
		}
	}).wait();

	return returnVal;
}

const string RestBase::readConfig(const string& arg) {
	string line;
	ifstream file(".\\config.json");
	stringstream content;

	if (file.is_open()) {
		while (getline(file, line)) {
			content << line << "\n";
		}
	}

	return utility::conversions::to_utf8string(
		json::value::parse(content)[utility::conversions::to_string_t(arg)].as_string());
}
