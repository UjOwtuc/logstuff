#ifndef TIMEINPUTDIALOG_H
#define TIMEINPUTDIALOG_H

#include <QDialog>
#include <QDateTime>

namespace Ui {
	class TimeInputDialog;
	class TimeInputWidget;
}

class TimeSpec
{
public:
	enum Kind {Absolute, Relative};
	enum Unit { Minutes=60, Hours=3600, Days=3600*24, Weeks=3600*24*7, Months=3600*24*30, Years=3600*24*365 };

	TimeSpec()
		: m_kind(Relative), m_relativeValue(0), m_relativeUnit(Hours)
	{}

	explicit TimeSpec(const QDateTime& absolute)
		: m_kind(Absolute), m_absolute(absolute)
	{}

	TimeSpec(int value, Unit unit)
		: m_kind(Relative), m_relativeValue(value), m_relativeUnit(unit)
	{}

	QStringList serialize() const;
	static TimeSpec deserialize(const QStringList& s);

	bool operator==(const TimeSpec& rhs) const;

	QString toString() const;
	QDateTime toDateTime() const;

	Kind kind() const { return m_kind; }
	int relativeValue() const { return m_relativeValue; }
	Unit relativeUnit() const { return m_relativeUnit; }

private:
	Kind m_kind;
	QDateTime m_absolute;
	int m_relativeValue;
	Unit m_relativeUnit;

};
Q_DECLARE_METATYPE(TimeSpec);


class TimeInputDialog : public QDialog
{
public:
	explicit TimeInputDialog(QWidget* parent = nullptr, Qt::WindowFlags f = Qt::WindowFlags());

	TimeSpec startTime() const;
	TimeSpec endTime() const;

private:
	Ui::TimeInputDialog* m_dialog;
	Ui::TimeInputWidget* m_start;
	Ui::TimeInputWidget* m_end;

	static TimeSpec widgetToTimeSpec(Ui::TimeInputWidget* widget);
};

#endif // TIMEINPUTDIALOG_H
