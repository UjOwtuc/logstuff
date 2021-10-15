#include "timerangemodel.h"
#include "timeinputdialog.h"

#include <QApplication>
#include <QDebug>

TimerangeModel::TimerangeModel(QObject* parent)
	: QAbstractListModel(parent)
{}


int TimerangeModel::rowCount(const QModelIndex& parent) const
{
	return m_data.size() +1;
}


QVariant TimerangeModel::headerData(int section, Qt::Orientation orientation, int role) const
{
	if (section == 0 && orientation == Qt::Horizontal && role == Qt::DisplayRole)
		return "Time Range";
	return QVariant();
}


QVariant TimerangeModel::data(const QModelIndex& index, int role) const
{
	QVariant result;
	if (role == Qt::DisplayRole && index.row() >= 0)
	{
		if (index.row() < m_data.size())
		{
			auto data = m_data[index.row()];
			result.setValue(QString("%1 to %2").arg(data.first.toString()).arg(data.second.toString()));
		}
		else
			result.setValue(QString("Custom ..."));
	}
	else if (role == Qt::UserRole && index.row() >= 0 && index.row() < m_data.size())
		result.setValue(m_data[index.row()]);
	return result;
}


int TimerangeModel::addChoice(const TimeSpec& start, const TimeSpec& end)
{
	auto entry = qMakePair(start, end);
	if (m_data.contains(entry))
	{
		return m_data.indexOf(entry);
	}
	else
	{
		beginInsertRows(QModelIndex(), m_data.size(), m_data.size());
		m_data.append(qMakePair(start, end));
		endInsertRows();
		return m_data.size() -1;
	}
}
