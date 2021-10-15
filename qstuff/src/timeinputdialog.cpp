#include "timeinputdialog.h"

#include "ui_timeinputdialog.h"
#include "ui_timeinputwidget.h"


TimeInputDialog::TimeInputDialog(QWidget* parent, Qt::WindowFlags f)
	: QDialog(parent, f)
{
	m_dialog = new Ui::TimeInputDialog;
	m_dialog->setupUi(this);

	m_start = new Ui::TimeInputWidget;
	m_start->setupUi(m_dialog->startWidget);

	m_end = new Ui::TimeInputWidget;
	m_end->setupUi(m_dialog->endWidget);

	QDateTime now = QDateTime::currentDateTime();
	m_start->calendarWidget->setCurrentPage(now.date().year(), now.date().month());
	m_start->timeEdit->setTime(QTime(now.time().hour() -1, 0, 0));
	m_end->calendarWidget->setCurrentPage(now.date().year(), now.date().month());
	m_end->timeEdit->setTime(QTime(now.time().hour() +1, 0, 0));

	m_start->numberBox->setValue(1);
	m_end->numberBox->setValue(0);
}


TimeSpec TimeInputDialog::widgetToTimeSpec(Ui::TimeInputWidget* widget)
{
	if (widget->tabWidget->currentIndex() == 0)
	{
		QDateTime selected;
		selected.setDate(widget->calendarWidget->selectedDate());
		selected.setTime(widget->timeEdit->time());
		return TimeSpec(selected);
	}
	TimeSpec::Unit unit;
	switch (widget->comboBox->currentIndex())
	{
		case 0:
			unit = TimeSpec::Minutes;
			break;
		case 1:
			unit = TimeSpec::Hours;
			break;
		case 2:
			unit = TimeSpec::Days;
			break;
		case 3:
			unit = TimeSpec::Weeks;
			break;
		case 4:
			unit = TimeSpec::Months;
			break;
		case 5:
			unit = TimeSpec::Years;
			break;
		default:
			qFatal("Unhandled index in relative start time unit selection");
	}
	return TimeSpec(widget->numberBox->value(), unit);
}


TimeSpec TimeInputDialog::startTime() const
{
	return widgetToTimeSpec(m_start);
}


TimeSpec TimeInputDialog::endTime() const
{
	return widgetToTimeSpec(m_end);
}


QStringList TimeSpec::serialize() const
{
	QStringList result;
	result.append((m_kind == Absolute) ? "absolute" : "relative");
	if (m_kind == Absolute)
		result.append(m_absolute.toUTC().toString(Qt::ISODate));
	else
	{
		result << QString::number(m_relativeValue) << QString::number(m_relativeUnit);
	}
	return result;
}


TimeSpec TimeSpec::deserialize(const QStringList& s)
{
	if (s.first() == "absolute")
		return TimeSpec(QDateTime::fromString(s[1], Qt::ISODate));
	else
	{
		int val = s[1].toInt();
		Unit unit = static_cast<Unit>(s[2].toInt());
		return TimeSpec(val, unit);
	}
	qFatal("Unhandled timespec format: %s", s.first().toStdString().c_str());
}


bool TimeSpec::operator==(const TimeSpec& rhs) const
{
	if (m_kind == Absolute)
	{
		if (rhs.m_kind == Absolute && m_absolute == rhs.m_absolute)
			return true;
	}
	else
	{
		if (rhs.m_kind == m_kind && m_relativeUnit == rhs.m_relativeUnit && m_relativeValue == rhs.m_relativeValue)
			return true;
	}
	return false;
}


QString TimeSpec::toString() const
{
	QString s;
	if (m_kind == Absolute)
		s = QLocale().toString(m_absolute, QLocale::ShortFormat);
	else if (m_relativeValue == 0)
		s = "now";
	else
	{
		QString unit;
		switch (m_relativeUnit)
		{
			case Minutes:
				unit = "minutes";
				break;
			case Hours:
				unit = "hours";
				break;
			case Days:
				unit = "days";
				break;
			case Weeks:
				unit = "weeks";
				break;
			case Months:
				unit = "months";
				break;
			case Years:
				unit = "years";
				break;
		}
		s = QString("%1 %2 ago").arg(m_relativeValue).arg(unit);
	}
	return s;
}


QDateTime TimeSpec::toDateTime() const
{
	if (m_kind == Absolute)
		return m_absolute;

	QDateTime now = QDateTime::currentDateTime();
	return now.addSecs(-1 * m_relativeValue * m_relativeUnit);
}
